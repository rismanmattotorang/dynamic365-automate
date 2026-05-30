//! D365-Automate MCP server binary.
//!
//! Wires the Dynamics 365 backends (mock by default) into an MCP server
//! exposing OData/Metadata tools, resources, and prompts over stdio or HTTP.
//!
//! Read-only by default; `--enable-writes` (or `D365_AUTOMATE_ENABLE_WRITES=1`)
//! exposes the gated write tools. The backends are held behind trait objects,
//! so pointing at a live environment later is a one-site change (see lib.rs).

use d365_automate_server_lib::{context::ServerContext, prompts, resources, seed, tools, register_completers};

use clap::Parser;
use mcp_server::Server;
use mcp_transport::{HttpServerConfig, HttpServerTransport, StdioTransport};
use d365_automate_graph::InMemoryGraph;
use d365_automate_ingest::{EmbeddingClient, MockEmbedder};
use d365_automate_kb::{InMemoryKb, KnowledgeStore};
use d365_automate_meta::{D365Connection, MetadataClient, MockMetadataClient};
use d365_automate_observability::{
    AuditEntry, AuditLog, AuditSink, MetricKind, MetricsRegistry,
};
use d365_automate_odata::{
    CredentialProvider, CredentialSource, Credentials, EnvCredentialProvider,
    LayeredCredentialProvider, MetadataCache, MockD365Client, StaticCredentialProvider,
    D365Client,
};
use d365_automate_rag::{GraphEngine, MockReranker, RagEngine};
use d365_automate_skills::SkillRegistry;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Clone)]
#[command(
    name = "d365-automate-server",
    about = "D365-Automate MCP server with Dynamics 365 OData/Metadata tools, resources, and prompts.",
    version,
)]
struct Cli {
    /// Disable read-only safety; allow MCP tools to call write-side OData
    /// operations. Equivalent to `D365_AUTOMATE_ENABLE_WRITES=1`.
    #[arg(long)]
    enable_writes: bool,

    /// Maximum concurrent Dynamics 365 calls.
    #[arg(long, default_value_t = 8)]
    pool_size: usize,

    /// Path to an AGENTS.md guardrails file. Surfaced in
    /// `initialize.instructions`. Defaults to `./AGENTS.md` if present.
    #[arg(long)]
    agents_md: Option<String>,

    /// Embedding vector dimension for the in-memory KB.
    #[arg(long, default_value_t = 256)]
    embedding_dim: usize,

    /// Metadata connection name (for a live environment; mock when unset).
    #[arg(long)]
    connection: Option<String>,

    /// Transport: "stdio" (default) or "http".
    #[arg(long, default_value = "stdio")]
    transport: String,

    /// HTTP listener bind address (used when --transport=http).
    #[arg(long, default_value = "127.0.0.1:3030")]
    bind: String,

    /// Optional bearer token required for HTTP requests.
    #[arg(long)]
    bearer_token: Option<String>,

    /// Allowed `Origin` header values for HTTP transport (DNS-rebinding
    /// mitigation). Repeatable. Empty disables the check.
    #[arg(long = "allowed-origin", num_args = 1)]
    allowed_origins: Vec<String>,

    /// TTL in seconds for the service-metadata cache. `0` disables caching.
    #[arg(long, default_value_t = 300)]
    metadata_cache_ttl_secs: u64,
}

/// Audit sink emitting each entry as a JSON line on the `d365_audit` tracing
/// target (stderr). Safe for stdio (stdout is the MCP channel).
struct TracingAuditSink;

#[async_trait::async_trait]
impl AuditSink for TracingAuditSink {
    async fn write(&self, entry: &AuditEntry) {
        let json = serde_json::to_string(entry).unwrap_or_else(|_| "{}".into());
        tracing::info!(target: "d365_audit", audit = %json, "state-mutating tool call");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let read_only = !cli.enable_writes
        && std::env::var("D365_AUTOMATE_ENABLE_WRITES").ok().as_deref() != Some("1");

    // KB + embedder + RAG.
    let store: Arc<dyn KnowledgeStore> = Arc::new(InMemoryKb::new());
    let embedder: Arc<dyn EmbeddingClient> = Arc::new(MockEmbedder::new(cli.embedding_dim));
    seed::populate_with_embeddings(&store, embedder.as_ref()).await?;
    let rag = Arc::new(RagEngine::new(store.clone()).with_reranker(Arc::new(MockReranker::new())));

    // Cross-domain knowledge graph.
    let kg = Arc::new(InMemoryGraph::with_demo_corpus());
    let graph_engine = Arc::new(GraphEngine::new(kg));
    tracing::info!(
        nodes = graph_engine.graph.stats().node_count,
        edges = graph_engine.graph.stats().edge_count,
        communities = graph_engine.communities.communities.len(),
        "graph engine ready"
    );

    // Entra ID credentials: layered (env first, static fallback for the demo).
    let creds_provider = LayeredCredentialProvider::new()
        .add(Arc::new(EnvCredentialProvider::new()))
        .add(Arc::new(StaticCredentialProvider::new(Credentials {
            resource: "https://gt-dev.operations.dynamics.com".into(),
            tenant_id: "00000000-0000-0000-0000-000000000000".into(),
            client_id: "demo-app".into(),
            client_secret: "redacted".into(),
            legal_entity: "USMF".into(),
            source: CredentialSource::Static,
        })));
    let creds = creds_provider.fetch().await
        .map_err(|e| anyhow::anyhow!("credential resolution failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("no credentials available"))?;
    tracing::info!(identity = %creds.redacted(), "Dynamics 365 identity resolved");

    // ── Dynamics 365 backends (mock today; live HttpD365Client swaps in here
    //    once Phase 3b lands — same Arc<dyn …> assignment, no tool changes). ──
    let inner = MockD365Client::new(cli.pool_size, creds.redacted());
    let cache_ttl = Duration::from_secs(cli.metadata_cache_ttl_secs);
    let metadata_cache = MetadataCache::new(inner.clone(), cache_ttl);
    let d365_client: Arc<dyn D365Client> = metadata_cache.clone();
    tracing::info!(cache_ttl_secs = cli.metadata_cache_ttl_secs, "service-metadata cache active");

    let connection = match &cli.connection {
        Some(name) => D365Connection::mock(name.clone()), // live loader lands in Phase 3b
        None => D365Connection::mock("default"),
    };
    let meta_client: Arc<dyn MetadataClient> = MockMetadataClient::new(connection);

    let agents_md = load_agents_md(cli.agents_md.as_deref()).await;
    let skills = SkillRegistry::new();

    let ctx = Arc::new(ServerContext {
        rag,
        graph: graph_engine,
        embedder,
        d365_client,
        metadata_cache: Some(metadata_cache),
        meta_client,
        read_only,
        agents_md: agents_md.clone(),
        audit: Arc::new(AuditLog::new(Arc::new(TracingAuditSink))),
        environment: Some("gt-dev/USMF".into()),
    });

    let server = build_server(ctx.clone(), &agents_md, read_only, &skills);

    tracing::info!(
        read_only = read_only,
        transport = %cli.transport,
        "D365-Automate server configured"
    );

    match cli.transport.as_str() {
        "stdio" => {
            let (reader, writer) = StdioTransport::new(
                tokio::io::stdin(), tokio::io::stdout(),
            ).into_parts();
            server.run_stdio(reader, writer).await?
        }
        "http" => {
            let bind: std::net::SocketAddr = cli.bind.parse()
                .map_err(|e| anyhow::anyhow!("invalid --bind '{}': {e}", cli.bind))?;
            tracing::info!(bind = %bind, "HTTP transport binding");

            let metrics = Arc::new(MetricsRegistry::new());
            metrics.register("mcp_tool_latency_seconds", MetricKind::Histogram, "Per-tool call latency in seconds");
            metrics.register("mcp_tool_calls_total", MetricKind::Counter, "Total MCP tool invocations");
            metrics.register("mcp_tool_errors_total", MetricKind::Counter, "Total MCP tool invocations that returned isError=true");
            metrics.register("rag_retrieval_latency_seconds", MetricKind::Histogram, "RAG retrieval latency");
            metrics.register("kb_chunks_total", MetricKind::Gauge, "Total chunks currently indexed");
            metrics.register("d365_pool_in_use", MetricKind::Gauge, "Dynamics 365 connection pool slots currently in use");
            metrics.register("d365_authz_denied_total", MetricKind::Counter, "Calls denied by the read-only safety gate");
            metrics.register("d365_service_calls_total", MetricKind::Counter, "OData operations dispatched, grouped by operation and outcome");

            let metrics_for_render = Arc::clone(&metrics);
            let render: mcp_transport::http::MetricsRenderFn = Arc::new(move || metrics_for_render.render());

            let dispatch_server = server.clone();
            let handle = HttpServerTransport::serve(
                HttpServerConfig {
                    bind,
                    bearer_token: cli.bearer_token.clone(),
                    metrics_renderer: Some(render),
                    allowed_origins: cli.allowed_origins.clone(),
                },
                move |msg| {
                    let server = dispatch_server.clone();
                    async move { server.dispatch_message(msg).await }
                },
            ).await?;
            tracing::info!("HTTP server ready at http://{bind}/mcp  (events: /mcp/events, metrics: /metrics)");
            tokio::signal::ctrl_c().await?;
            handle.shutdown().await;
        }
        other => anyhow::bail!("unknown --transport '{other}' (expected: stdio | http)"),
    }
    Ok(())
}

/// Register every tool group, resource, and prompt under the exposure policy.
fn build_server(
    ctx: Arc<ServerContext>,
    agents_md: &Option<String>,
    read_only: bool,
    skills: &SkillRegistry,
) -> Server {
    let policy = if read_only {
        mcp_server::ExposurePolicy::ReadOnlyOnly
    } else {
        mcp_server::ExposurePolicy::All
    };
    let instructions = agents_md.clone().unwrap_or_else(|| {
        "D365-Automate MCP server. Read-only by default; cite every claim by source URI.".into()
    });
    let mut builder = Server::builder("d365-automate-server", env!("CARGO_PKG_VERSION"))
        .exposure(policy)
        .instructions(instructions);

    for desc in tools::rag_tools(&ctx) { builder = builder.tool(desc); }
    for desc in tools::service_tools(&ctx) { builder = builder.tool(desc); }
    for desc in tools::meta_tools(&ctx) { builder = builder.tool(desc); }
    for desc in tools::graph_tools(&ctx) { builder = builder.tool(desc); }
    for desc in tools::workflow_tools(&ctx) { builder = builder.tool(desc); }
    for desc in resources::all(&ctx) { builder = builder.resource(desc); }
    for desc in prompts::all(skills) { builder = builder.prompt(desc); }
    builder = register_completers(builder);
    builder.build()
}

/// Load AGENTS.md from an explicit path or `./AGENTS.md` if present.
async fn load_agents_md(explicit_path: Option<&str>) -> Option<String> {
    let path = explicit_path.map(|p| p.to_string()).or_else(|| {
        let default = "AGENTS.md";
        std::path::Path::new(default).exists().then(|| default.to_string())
    })?;
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => {
            tracing::info!(path = %path, "loaded AGENTS.md guardrails");
            Some(s)
        }
        Err(e) => {
            tracing::warn!(path = %path, error = %e, "failed to read AGENTS.md");
            None
        }
    }
}
