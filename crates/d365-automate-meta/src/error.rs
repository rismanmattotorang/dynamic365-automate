//! Structured Metadata API error taxonomy.
//!
//! Dynamics 365 Metadata / X++ failure modes are split into typed variants so
//! the MCP layer can map each to its JSON-RPC error code. Ported from the
//! SAP-Automate ADT taxonomy.

use thiserror::Error;

pub type MetaResult<T> = std::result::Result<T, MetaError>;

/// Structured error codes for Dynamics 365 Metadata operations. Numeric values
/// are stable across releases; `#[non_exhaustive]` lets us add variants without
/// breaking downstream matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MetaErrorCode {
    // Transient (-32100..-32199)
    Timeout = -32160,
    EnvironmentDown = -32161,
    TokenRefresh = -32162,
    RateLimited = -32163,

    // Permanent (-32200..-32299)
    AuthFailed = -32260,
    NotFound = -32261,
    Forbidden = -32262,
    InvalidObjectName = -32263,
    NotDeployed = -32264,
    /// Entity data read blocked by environment policy (analog of ADT data
    /// preview being blocked on BTP).
    EntityDataBlocked = -32265,
    PermissionDenied = -32266,
    /// Object exists but is locked (checked out, in an undeployed package, etc.).
    Locked = -32267,
    /// Server bug / programming error.  Never retried.
    Internal = -32298,
}

impl MetaErrorCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
    pub fn is_transient(self) -> bool {
        let v = self as i32;
        (-32199..=-32100).contains(&v)
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MetaError {
    #[error("Metadata request timeout after {timeout_ms} ms")]
    Timeout { timeout_ms: u64 },

    #[error("environment '{environment}' unreachable: {reason}")]
    EnvironmentDown { environment: String, reason: String },

    #[error("Entra ID token refresh required")]
    TokenRefresh,

    #[error("rate limited; retry after {retry_after_ms} ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("object not found: {kind} '{name}'")]
    NotFound { kind: String, name: String },

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("invalid object name '{0}'")]
    InvalidObjectName(String),

    #[error("object is not deployed: {0}")]
    NotDeployed(String),

    #[error("entity data read blocked by environment policy: {0}")]
    EntityDataBlocked(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("object locked: {0}")]
    Locked(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl MetaError {
    pub fn code(&self) -> MetaErrorCode {
        match self {
            MetaError::Timeout { .. } => MetaErrorCode::Timeout,
            MetaError::EnvironmentDown { .. } => MetaErrorCode::EnvironmentDown,
            MetaError::TokenRefresh => MetaErrorCode::TokenRefresh,
            MetaError::RateLimited { .. } => MetaErrorCode::RateLimited,
            MetaError::AuthFailed(_) => MetaErrorCode::AuthFailed,
            MetaError::NotFound { .. } => MetaErrorCode::NotFound,
            MetaError::Forbidden(_) => MetaErrorCode::Forbidden,
            MetaError::InvalidObjectName(_) => MetaErrorCode::InvalidObjectName,
            MetaError::NotDeployed(_) => MetaErrorCode::NotDeployed,
            MetaError::EntityDataBlocked(_) => MetaErrorCode::EntityDataBlocked,
            MetaError::PermissionDenied(_) => MetaErrorCode::PermissionDenied,
            MetaError::Locked(_) => MetaErrorCode::Locked,
            // Programmer bugs are permanent — must NOT be retried.
            MetaError::Internal(_) => MetaErrorCode::Internal,
        }
    }

    pub fn is_transient(&self) -> bool {
        self.code().is_transient()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_is_permanent() {
        let e = MetaError::Internal("bug".into());
        assert!(
            !e.is_transient(),
            "MetaError::Internal must be permanent to prevent retry-loop on programmer bugs"
        );
    }

    #[test]
    fn transient_kinds_are_classified_correctly() {
        for code in [
            MetaErrorCode::Timeout,
            MetaErrorCode::EnvironmentDown,
            MetaErrorCode::TokenRefresh,
            MetaErrorCode::RateLimited,
        ] {
            assert!(code.is_transient(), "{code:?} should be transient");
        }
        for code in [
            MetaErrorCode::AuthFailed,
            MetaErrorCode::NotFound,
            MetaErrorCode::Forbidden,
            MetaErrorCode::Internal,
            MetaErrorCode::EntityDataBlocked,
            MetaErrorCode::Locked,
        ] {
            assert!(!code.is_transient(), "{code:?} should NOT be transient");
        }
    }
}
