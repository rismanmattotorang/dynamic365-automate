//! MCP resources.
//!
//! Read-only resources published at startup so MCP clients see them in
//! `resources/list` and can fetch them via `resources/read`:
//!   - `d365-env://info`                  — environment identity
//!   - `d365-entity://{name}/structure`   — data-entity structure (one per seeded entity)
//!   - `d365-service://{name}`            — OData operation metadata (one per seeded operation)
//!   - `d365-meta://info`                 — Metadata connection summary (redacted)
//!   - `d365-cache://stats`               — service-metadata cache counters
//!   - `agents://guardrails`              — the loaded AGENTS.md, if any

use crate::context::ServerContext;
use mcp_core::{Error, ReadResourceResult, Resource, ResourceContents};
use mcp_server::{registry::ResourceHandler, ResourceDescriptor};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub fn all(ctx: &Arc<ServerContext>) -> Vec<ResourceDescriptor> {
    let mut out = Vec::new();
    out.push(make_env_info(ctx));

    for entity in &["CompaniesV2", "ReleasedProductsV2", "LedgerJournalTrans", "GeneralJournalAccountEntry"] {
        out.push(make_entity_structure(ctx, entity));
    }
    for op in &[
        "EnvironmentInfo", "ReleasedProductGetDetail", "LedgerGeneralJournalEntryPost",
        "PurchaseOrderCreate", "SalesOrderCreate", "CustomerMaintain",
    ] {
        out.push(make_service_meta(ctx, op));
    }

    out.push(make_meta_connection(ctx));
    if ctx.metadata_cache.is_some() { out.push(make_cache_stats(ctx)); }
    if ctx.agents_md.is_some() { out.push(make_agents_md(ctx)); }
    out
}

fn json_result(uri: String, text: String) -> ReadResourceResult {
    ReadResourceResult {
        contents: vec![ResourceContents {
            uri, mime_type: Some("application/json".into()), text: Some(text), blob: None,
        }],
    }
}

fn make_env_info(ctx: &Arc<ServerContext>) -> ResourceDescriptor {
    struct H(Arc<ServerContext>);
    impl ResourceHandler for H {
        fn read(&self, uri: &str) -> Pin<Box<dyn Future<Output = mcp_core::Result<ReadResourceResult>> + Send + '_>> {
            let uri = uri.to_string();
            let ctx = Arc::clone(&self.0);
            Box::pin(async move {
                let info = ctx.d365_client.environment_info().await.map_err(|e| Error::Other(e.to_string()))?;
                let text = serde_json::to_string_pretty(&info).map_err(Error::Json)?;
                Ok(json_result(uri, text))
            })
        }
    }
    ResourceDescriptor {
        resource: Resource {
            uri: "d365-env://info".into(),
            name: "Dynamics 365 environment identity".into(),
            description: Some("Live environment, legal entity, version, base URL, and identity. JSON.".into()),
            mime_type: Some("application/json".into()),
        },
        handler: Arc::new(H(Arc::clone(ctx))),
    }
}

fn make_entity_structure(ctx: &Arc<ServerContext>, entity: &str) -> ResourceDescriptor {
    struct H { ctx: Arc<ServerContext>, entity: String }
    impl ResourceHandler for H {
        fn read(&self, uri: &str) -> Pin<Box<dyn Future<Output = mcp_core::Result<ReadResourceResult>> + Send + '_>> {
            let uri = uri.to_string();
            let ctx = Arc::clone(&self.ctx);
            let entity = self.entity.clone();
            Box::pin(async move {
                let s = ctx.d365_client.entity_structure(&entity).await.map_err(|e| Error::Other(e.to_string()))?;
                let text = serde_json::to_string_pretty(&s).map_err(Error::Json)?;
                Ok(json_result(uri, text))
            })
        }
    }
    ResourceDescriptor {
        resource: Resource {
            uri: format!("d365-entity://{entity}/structure"),
            name: format!("Structure of {entity}"),
            description: Some(format!("Field metadata for data entity {entity}.")),
            mime_type: Some("application/json".into()),
        },
        handler: Arc::new(H { ctx: Arc::clone(ctx), entity: entity.into() }),
    }
}

fn make_service_meta(ctx: &Arc<ServerContext>, operation: &str) -> ResourceDescriptor {
    struct H { ctx: Arc<ServerContext>, operation: String }
    impl ResourceHandler for H {
        fn read(&self, uri: &str) -> Pin<Box<dyn Future<Output = mcp_core::Result<ReadResourceResult>> + Send + '_>> {
            let uri = uri.to_string();
            let ctx = Arc::clone(&self.ctx);
            let operation = self.operation.clone();
            Box::pin(async move {
                let meta = ctx.d365_client.service_metadata(&operation, "en-us").await.map_err(|e| Error::Other(e.to_string()))?;
                let text = serde_json::to_string_pretty(&meta).map_err(Error::Json)?;
                Ok(json_result(uri, text))
            })
        }
    }
    ResourceDescriptor {
        resource: Resource {
            uri: format!("d365-service://{operation}"),
            name: format!("Service metadata: {operation}"),
            description: Some(format!("Parameter signature, read-only flag, and security for {operation}.")),
            mime_type: Some("application/json".into()),
        },
        handler: Arc::new(H { ctx: Arc::clone(ctx), operation: operation.into() }),
    }
}

fn make_meta_connection(ctx: &Arc<ServerContext>) -> ResourceDescriptor {
    struct H(Arc<ServerContext>);
    impl ResourceHandler for H {
        fn read(&self, uri: &str) -> Pin<Box<dyn Future<Output = mcp_core::Result<ReadResourceResult>> + Send + '_>> {
            let uri = uri.to_string();
            let conn = self.0.meta_client.connection().redacted();
            Box::pin(async move {
                let text = serde_json::to_string_pretty(&conn).map_err(Error::Json)?;
                Ok(json_result(uri, text))
            })
        }
    }
    ResourceDescriptor {
        resource: Resource {
            uri: "d365-meta://info".into(),
            name: "Metadata connection".into(),
            description: Some("Redacted view of the configured Metadata API connection (name, base URL, legal entity, auth type).".into()),
            mime_type: Some("application/json".into()),
        },
        handler: Arc::new(H(Arc::clone(ctx))),
    }
}

fn make_cache_stats(ctx: &Arc<ServerContext>) -> ResourceDescriptor {
    struct H(Arc<ServerContext>);
    impl ResourceHandler for H {
        fn read(&self, uri: &str) -> Pin<Box<dyn Future<Output = mcp_core::Result<ReadResourceResult>> + Send + '_>> {
            let uri = uri.to_string();
            let ctx = Arc::clone(&self.0);
            Box::pin(async move {
                let cache = ctx.metadata_cache.as_ref().ok_or_else(|| Error::Other("metadata cache disabled".into()))?;
                let s = cache.stats().await;
                let text = serde_json::to_string_pretty(&serde_json::json!({
                    "hits": s.hits, "misses": s.misses, "entries": s.entries,
                    "evictions": s.evictions, "hit_ratio": s.hit_ratio(),
                })).map_err(Error::Json)?;
                Ok(json_result(uri, text))
            })
        }
    }
    ResourceDescriptor {
        resource: Resource {
            uri: "d365-cache://stats".into(),
            name: "Service metadata cache stats".into(),
            description: Some("Live hit/miss counters for the service-metadata cache. JSON.".into()),
            mime_type: Some("application/json".into()),
        },
        handler: Arc::new(H(Arc::clone(ctx))),
    }
}

fn make_agents_md(ctx: &Arc<ServerContext>) -> ResourceDescriptor {
    struct H(Arc<ServerContext>);
    impl ResourceHandler for H {
        fn read(&self, uri: &str) -> Pin<Box<dyn Future<Output = mcp_core::Result<ReadResourceResult>> + Send + '_>> {
            let uri = uri.to_string();
            let text = self.0.agents_md.clone().unwrap_or_default();
            Box::pin(async move {
                Ok(ReadResourceResult {
                    contents: vec![ResourceContents {
                        uri, mime_type: Some("text/markdown".into()), text: Some(text), blob: None,
                    }],
                })
            })
        }
    }
    ResourceDescriptor {
        resource: Resource {
            uri: "agents://guardrails".into(),
            name: "Agent guardrails".into(),
            description: Some("Project-local AGENTS.md, surfaced from disk on server start.".into()),
            mime_type: Some("text/markdown".into()),
        },
        handler: Arc::new(H(Arc::clone(ctx))),
    }
}
