//! `MetadataClient` async trait.
//!
//! Every method that modifies state takes a `MetaCallContext` carrying the
//! server's read-only-mode flag. Mock and (future) HTTP backends both honour
//! it, refusing writes when set. The Dynamics 365 analog of `AdtClient`.

use crate::connection::D365Connection;
use crate::error::MetaResult;
use crate::types::{
    CrossReferenceHit, CrossReferenceRequest, DataEntityView, DeployOutcome, DeployRequest,
    EntityRow, MetaSearchHit, MetaSearchRequest, ModelContents, ObjectSource,
};
use async_trait::async_trait;

/// Per-call security / observability context.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetaCallContext {
    pub read_only: bool,
}

#[async_trait]
pub trait MetadataClient: Send + Sync {
    /// Connection metadata (redacted form is safe for logs).
    fn connection(&self) -> &D365Connection;

    // --- Read-only ---------------------------------------------------------

    async fn get_class(&self, name: &str) -> MetaResult<ObjectSource>;
    async fn get_interface(&self, name: &str) -> MetaResult<ObjectSource>;
    async fn get_table(&self, name: &str) -> MetaResult<ObjectSource>;
    async fn get_job(&self, name: &str) -> MetaResult<ObjectSource>;
    async fn get_form(&self, name: &str) -> MetaResult<ObjectSource>;
    async fn get_model_contents(&self, model: &str) -> MetaResult<ModelContents>;
    async fn get_data_entity(&self, name: &str) -> MetaResult<DataEntityView>;

    async fn search(&self, request: MetaSearchRequest) -> MetaResult<Vec<MetaSearchHit>>;
    async fn cross_reference(
        &self,
        request: CrossReferenceRequest,
    ) -> MetaResult<Vec<CrossReferenceHit>>;

    /// Read data-entity contents. Some environments block bulk entity reads
    /// at policy level; the call then returns `MetaError::EntityDataBlocked`
    /// so the agent can fall back to the OData path (`d365.entity.read`).
    async fn get_entity_contents(
        &self,
        entity: &str,
        max_rows: usize,
    ) -> MetaResult<Vec<EntityRow>>;

    // --- Write (gated by `ctx.read_only`) ---------------------------------

    /// Build & deploy an object (the analog of ADT `activate`).
    async fn deploy(
        &self,
        request: DeployRequest,
        ctx: MetaCallContext,
    ) -> MetaResult<DeployOutcome>;
}
