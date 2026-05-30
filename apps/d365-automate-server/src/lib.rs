//! Library surface for the D365-Automate server binary.
//!
//! Exposes the same builder the binary uses so integration tests can construct
//! a server in-process — no subprocess, no cold-start seeding cost.
//!
//! **Swapping in production:** every backend is held behind a trait object
//! (`Arc<dyn D365Client>`, `Arc<dyn MetadataClient>`). Today they are the
//! offline mocks (`MockD365Client`, `MockMetadataClient`); pointing the server
//! at a live Dynamics 365 environment is a one-site change here — construct the
//! live `HttpD365Client` / live Metadata client (Phase 3b) instead of the mock
//! and assign it to the same `Arc<dyn …>`. No tool, resource, or prompt code
//! changes.

pub mod context;
pub mod prompts;
pub mod resources;
pub mod seed;
pub mod tools;

use std::sync::Arc;
use std::time::Duration;

use mcp_server::Server;
use d365_automate_graph::InMemoryGraph;
use d365_automate_ingest::{EmbeddingClient, MockEmbedder};
use d365_automate_kb::{InMemoryKb, KnowledgeStore};
use d365_automate_meta::{D365Connection, MetadataClient, MockMetadataClient};
use d365_automate_observability::{AuditLog, JsonStderrSink};
use d365_automate_odata::{D365Client, MetadataCache, MockD365Client};
use d365_automate_rag::{GraphEngine, MockReranker, RagEngine};
use d365_automate_skills::SkillRegistry;

pub use context::ServerContext;

/// How the test harness wants its context built.
#[derive(Clone)]
pub struct TestServerOptions {
    pub read_only: bool,
    pub metadata_cache_ttl: Duration,
    pub seed_kb: bool,
    pub embedding_dim: usize,
    pub agents_md: Option<String>,
}

impl Default for TestServerOptions {
    fn default() -> Self {
        Self {
            read_only: true,
            metadata_cache_ttl: Duration::from_secs(300),
            seed_kb: false,
            embedding_dim: 64,
            agents_md: None,
        }
    }
}

/// Build a ready-to-run `Server`. Identical wiring to `main.rs`, minus the
/// network transport setup and (optionally) the KB seed step.
pub async fn build_test_server(opts: TestServerOptions) -> (Server, Arc<ServerContext>) {
    let store: Arc<dyn KnowledgeStore> = Arc::new(InMemoryKb::new());
    let embedder: Arc<dyn EmbeddingClient> = Arc::new(MockEmbedder::new(opts.embedding_dim));
    if opts.seed_kb {
        seed::populate_with_embeddings(&store, embedder.as_ref()).await.expect("seed");
    }
    let rag = Arc::new(RagEngine::new(store.clone()).with_reranker(Arc::new(MockReranker::new())));

    let kg = Arc::new(InMemoryGraph::with_demo_corpus());
    let graph_engine = Arc::new(GraphEngine::new(kg));

    // ── Dynamics 365 backends (mock today; live client swaps in here) ──
    let inner = MockD365Client::new(4, serde_json::json!({ "legal_entity": "USMF" }));
    let metadata_cache = MetadataCache::new(inner.clone(), opts.metadata_cache_ttl);
    let d365_client: Arc<dyn D365Client> = metadata_cache.clone();

    let connection = D365Connection::mock("test".to_string());
    let meta_client: Arc<dyn MetadataClient> = MockMetadataClient::new(connection);

    let ctx = Arc::new(ServerContext {
        rag,
        graph: graph_engine,
        embedder,
        d365_client,
        metadata_cache: Some(metadata_cache),
        meta_client,
        read_only: opts.read_only,
        agents_md: opts.agents_md.clone(),
        audit: Arc::new(AuditLog::new(Arc::new(JsonStderrSink::new()))),
        environment: Some("gt-dev/USMF".into()),
    });

    let policy = if opts.read_only {
        mcp_server::ExposurePolicy::ReadOnlyOnly
    } else {
        mcp_server::ExposurePolicy::All
    };
    let mut builder = Server::builder("d365-automate-test-server", env!("CARGO_PKG_VERSION"))
        .exposure(policy)
        .instructions("integration test".to_string());

    for desc in tools::rag_tools(&ctx) { builder = builder.tool(desc); }
    for desc in tools::service_tools(&ctx) { builder = builder.tool(desc); }
    for desc in tools::meta_tools(&ctx) { builder = builder.tool(desc); }
    for desc in tools::graph_tools(&ctx) { builder = builder.tool(desc); }
    for desc in tools::workflow_tools(&ctx) { builder = builder.tool(desc); }
    for desc in resources::all(&ctx) { builder = builder.resource(desc); }
    let skills = SkillRegistry::new();
    for desc in prompts::all(&skills) { builder = builder.prompt(desc); }
    builder = register_completers(builder);

    (builder.build(), ctx)
}

/// Register `completion/complete` providers for the prompt arguments that
/// benefit most from autocomplete in MCP clients.
pub fn register_completers(builder: mcp_server::ServerBuilder) -> mcp_server::ServerBuilder {
    let starts_with = |options: &[&'static str], prefix: &str| -> Vec<String> {
        let p = prefix.to_ascii_lowercase();
        options.iter()
            .filter(|o| o.to_ascii_lowercase().starts_with(&p))
            .map(|o| (*o).to_string())
            .collect()
    };
    builder
        // Cross-reference review: object kind enum.
        .completer("xpp.review-cross-reference", "kind", move |prefix, _| {
            starts_with(&["class", "interface", "table", "data_entity", "job", "form"], prefix)
        })
        // Deploy impact analysis: target environment.
        .completer("d365.deploy-impact-analysis", "scope", move |prefix, _| {
            starts_with(&["DEV", "UAT", "PRODUCTION"], prefix)
        })
}
