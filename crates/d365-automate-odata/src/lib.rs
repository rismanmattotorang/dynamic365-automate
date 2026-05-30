//! D365-Automate OData / Custom Service client abstraction.
//!
//! The Dynamics 365 Finance & Operations equivalent of SAP-Automate's RFC tier.
//! Where SAP exposed RFC/BAPI function modules and DDIC tables over the
//! NetWeaver stack, Dynamics 365 exposes **OData v4 actions / Custom Service
//! operations** and **data entities** over HTTPS, authenticated with Microsoft
//! Entra ID. The trait shape is preserved from `sap-automate-rfc` so the MCP
//! server tool layer ports with minimal churn.
//!
//! The crate is split into:
//! - `client`: the `D365Client` trait + `MockD365Client` (offline)
//! - `credentials`: layered Entra ID credential provider (env / static)
//! - `error`: structured error taxonomy mapped to MCP error codes
//! - `infolog`: OData error / Infolog parser (replaces the BAPIRET2 parser)
//! - `metadata_cache`: TTL decorator over any `D365Client`
//! - `pool`: tokio-semaphore-based concurrency limiter
//! - `retry`: exponential-backoff helper + circuit-breaker primitive
//! - `transaction`: atomic `$batch` change-set write orchestration
//!
//! The live F&O OData v4 HTTP client lands in Phase 3b behind the `http`
//! feature; the offline mock is the default so CI without an environment is
//! unaffected.

pub mod client;
pub mod credentials;
pub mod error;
pub mod infolog;
pub mod metadata_cache;
pub mod pool;
pub mod retry;
pub mod transaction;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "http")]
pub use http::{HttpD365Client, HttpD365Config};

pub use client::{
    BulkMetadata, D365Client, EntityField, EntityRow, EntityStructure, EnvironmentInfo,
    MockD365Client, ParamDirection, PoolStatus, ReadEntityRequest, SecurityReference,
    ServiceCallRequest, ServiceOperationMeta, ServiceParameter, ServiceSearchResult,
    ServiceSummary, COMPANY_FIELD, MAX_ROWS_HARD_CAP,
};
pub use credentials::{
    CredentialProvider, CredentialSource, Credentials, EnvCredentialProvider,
    LayeredCredentialProvider, StaticCredentialProvider,
};
pub use error::{D365Error, D365ErrorCode, D365Result};
pub use infolog::{parse_infolog, InfologMessage, InfologSeverity};
pub use metadata_cache::{CacheStats, MetadataCache};
pub use pool::ConnectionPool;
pub use retry::{retry_with_backoff, BackoffPolicy, CircuitBreaker, CircuitState};
pub use transaction::{execute_write_operation, has_failure, WriteOutcome};
