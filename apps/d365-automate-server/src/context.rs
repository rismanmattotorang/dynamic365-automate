//! Shared server context — held in an `Arc` and cloned into every tool.

use d365_automate_meta::MetadataClient;
use d365_automate_ingest::EmbeddingClient;
use d365_automate_observability::AuditLog;
use d365_automate_odata::{D365Client, MetadataCache, MockD365Client};
use d365_automate_rag::{GraphEngine, RagEngine};
use std::sync::Arc;

pub struct ServerContext {
    pub rag: Arc<RagEngine>,
    pub graph: Arc<GraphEngine>,
    pub embedder: Arc<dyn EmbeddingClient>,
    /// The cache-decorated `D365Client` used by every tool. Identical trait
    /// surface to the underlying `MockD365Client` and a future live
    /// `HttpD365Client`, so swapping in production touches only construction.
    pub d365_client: Arc<dyn D365Client>,
    /// Direct handle to the metadata cache for the cache-stats / invalidate
    /// tools and the `d365-cache://stats` resource. `None` when caching is
    /// disabled via `--metadata-cache-ttl-secs=0`.
    pub metadata_cache: Option<Arc<MetadataCache<MockD365Client>>>,
    /// X++ / AOT metadata client (mock by default; live Metadata API later).
    pub meta_client: Arc<dyn MetadataClient>,
    pub read_only: bool,
    pub agents_md: Option<String>,
    /// Append-only audit log for state-mutating tool calls. Arguments are
    /// redacted by `AuditLog::record`.
    pub audit: Arc<AuditLog>,
    /// Dynamics 365 environment identity recorded on each audit entry.
    pub environment: Option<String>,
}
