//! Structured Dynamics 365 OData error taxonomy.
//!
//! Every failure mode is typed and mapped to the MCP error taxonomy so
//! callers can distinguish transient (retry) from permanent (do not retry)
//! errors at the JSON-RPC layer. Ported from the SAP-Automate RFC taxonomy
//! and re-grounded in Dynamics 365 / OData semantics.

use thiserror::Error;

pub type D365Result<T> = std::result::Result<T, D365Error>;

/// Structured error codes for Dynamics 365 OData / Custom Service operations.
/// Numeric values overlap the MCP code ranges so they translate cleanly into
/// a JSON-RPC error object. `#[non_exhaustive]` lets us add variants in a
/// minor release without breaking exhaustive matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum D365ErrorCode {
    // Transient (-32100..-32199): retryable
    Timeout = -32110,
    EnvironmentDown = -32120,
    PoolExhausted = -32130,
    CircuitOpen = -32140,
    UpstreamRateLimit = -32150,

    // Permanent (-32200..-32299): do not retry
    AuthFailed = -32210,
    NotFound = -32220,
    QueryResultOverflow = -32230,
    InvalidParameter = -32240,
    PermissionDenied = -32250,
    SchemaViolation = -32260,
    /// Server bug / programming error.  Never retried.
    Internal = -32299,

    // Degraded (-32300..-32399): partial result
    PartialBulk = -32310,
    StaleMetadata = -32320,
}

impl D365ErrorCode {
    pub fn as_i32(self) -> i32 { self as i32 }

    /// Whether the caller should retry after backoff.
    pub fn is_transient(self) -> bool {
        let v = self as i32;
        (-32199..=-32100).contains(&v)
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum D365Error {
    #[error("OData request timeout after {timeout_ms} ms")]
    Timeout { timeout_ms: u64 },

    #[error("Dynamics 365 environment '{environment}' unreachable: {reason}")]
    EnvironmentDown { environment: String, reason: String },

    #[error("connection pool exhausted (cap={cap})")]
    PoolExhausted { cap: usize },

    #[error("circuit open until ~{retry_after_ms} ms from now")]
    CircuitOpen { retry_after_ms: u64 },

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("query result overflow for '{entity}' (max_rows={max_rows})")]
    QueryResultOverflow { entity: String, max_rows: usize },

    #[error("invalid parameter '{name}': {reason}")]
    InvalidParameter { name: String, reason: String },

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("schema violation: {0}")]
    SchemaViolation(String),

    #[error("partial result: {0}")]
    PartialBulk(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl D365Error {
    pub fn code(&self) -> D365ErrorCode {
        match self {
            D365Error::Timeout { .. } => D365ErrorCode::Timeout,
            D365Error::EnvironmentDown { .. } => D365ErrorCode::EnvironmentDown,
            D365Error::PoolExhausted { .. } => D365ErrorCode::PoolExhausted,
            D365Error::CircuitOpen { .. } => D365ErrorCode::CircuitOpen,
            D365Error::AuthFailed(_) => D365ErrorCode::AuthFailed,
            D365Error::NotFound(_) => D365ErrorCode::NotFound,
            D365Error::QueryResultOverflow { .. } => D365ErrorCode::QueryResultOverflow,
            D365Error::InvalidParameter { .. } => D365ErrorCode::InvalidParameter,
            D365Error::PermissionDenied(_) => D365ErrorCode::PermissionDenied,
            D365Error::SchemaViolation(_) => D365ErrorCode::SchemaViolation,
            D365Error::PartialBulk(_) => D365ErrorCode::PartialBulk,
            // Internal errors are programming bugs, not transient outages —
            // they must NOT be retried.
            D365Error::Internal(_) => D365ErrorCode::Internal,
        }
    }

    pub fn is_transient(&self) -> bool { self.code().is_transient() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_classification_only_matches_transient_range() {
        for c in [
            D365ErrorCode::Timeout, D365ErrorCode::EnvironmentDown,
            D365ErrorCode::PoolExhausted, D365ErrorCode::CircuitOpen,
            D365ErrorCode::UpstreamRateLimit,
        ] {
            assert!(c.is_transient(), "{c:?} should be transient");
        }
        for c in [
            D365ErrorCode::AuthFailed, D365ErrorCode::NotFound,
            D365ErrorCode::QueryResultOverflow, D365ErrorCode::InvalidParameter,
            D365ErrorCode::PermissionDenied, D365ErrorCode::SchemaViolation,
            D365ErrorCode::Internal,
            D365ErrorCode::PartialBulk, D365ErrorCode::StaleMetadata,
        ] {
            assert!(!c.is_transient(), "{c:?} should NOT be transient");
        }
    }

    #[test]
    fn internal_is_permanent() {
        let e = D365Error::Internal("bug".into());
        assert!(!e.is_transient());
        assert_eq!(e.code() as i32, D365ErrorCode::Internal as i32);
    }
}
