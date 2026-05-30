//! D365-Automate Metadata API / X++ (AOT) client.
//!
//! The Dynamics 365 equivalent of SAP-Automate's ABAP Development Tools tier.
//! Where ADT exposed ABAP objects over the `/sap/bc/adt` REST surface,
//! Dynamics 365 exposes X++ / AOT objects over the F&O **Metadata API**
//! (`/metadata`) and data entities over OData. The trait shape is preserved
//! from `sap-automate-adt` so the MCP server tool layer ports with minimal
//! churn.
//!
//! The crate is split into:
//!   - `types`      — request/response shapes shared by every backend
//!   - `client`     — the `MetadataClient` async trait + `MetaCallContext`
//!   - `mock`       — offline `MockMetadataClient` with realistic X++ fixtures
//!   - `error`      — structured `MetaError` taxonomy mapped to MCP codes
//!   - `connection` — connection model (name, base URL, Entra auth)
//!   - `http` (feature `http`) — live Metadata API client (Phase 3b)
//!
//! Read-only-by-default safety is enforced by `MetaCallContext::read_only`,
//! mirroring the `d365-automate-odata` pattern.

pub mod client;
pub mod connection;
pub mod error;
pub mod mock;
pub mod types;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "http")]
pub use http::{load_connection, HttpMetadataClient};

pub use client::{MetaCallContext, MetadataClient};
pub use connection::{D365Auth, D365Connection};
pub use error::{MetaError, MetaErrorCode, MetaResult};
pub use mock::MockMetadataClient;
pub use types::{
    CrossReferenceHit, CrossReferenceRequest, DataEntityView, DeployOutcome, DeployRequest,
    EntityRow, MetaSearchHit, MetaSearchRequest, ModelContents, ModelMember, ObjectSource,
    XppObjectKind, MAX_ENTITY_ROWS,
};
