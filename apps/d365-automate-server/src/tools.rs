//! Tool registrations for the Dynamics 365 server.
//!
//! Tool groups:
//!   - `rag_tools`     — hybrid search over the four D365 knowledge domains
//!   - `service_tools` — environment / OData service / data-entity tools
//!   - `meta_tools`    — X++ / AOT metadata tools (`xpp.meta.*`)
//!   - `graph_tools`   — GraphRAG / HippoRAG / RAPTOR traversal
//!   - `workflow_tools`— gated, elicitation-driven write workflows
//!
//! Every tool shares one `ServerContext`: one `D365Client`, one
//! `MetadataClient`, one `RagEngine`, one `EmbeddingClient`.

use crate::context::ServerContext;
use d365_automate_kb::Domain;
use d365_automate_meta::{
    CrossReferenceRequest, DeployRequest, MetaCallContext, MetaSearchRequest, XppObjectKind,
};
use d365_automate_observability::{AuditEntry, AuditLog, AuditOutcome};
use d365_automate_odata::{
    execute_write_operation, parse_infolog, D365Error, ReadEntityRequest, ServiceCallRequest,
    MAX_ROWS_HARD_CAP,
};
use d365_automate_rag::Query;
use mcp_core::{CallToolResult, ToolContent, ToolInputSchema};
use mcp_server::{registry::ToolFn, ToolDescriptor};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;

// ===========================================================================
// RAG tools — search the four Dynamics 365 knowledge domains
// ===========================================================================

pub fn rag_tools(ctx: &Arc<ServerContext>) -> Vec<ToolDescriptor> {
    vec![
        make_rag_tool(
            ctx,
            "xpp.search",
            "Hybrid search over the X++ source corpus.",
            Domain::Xpp,
        ),
        make_rag_tool(
            ctx,
            "flow.find_process",
            "Search the Power Automate / business-process repository.",
            Domain::Flow,
        ),
        make_rag_tool(
            ctx,
            "app.search_solutions",
            "Search the Dataverse solution / module fact sheets.",
            Domain::Solution,
        ),
        make_rag_tool(
            ctx,
            "d365.learn.search",
            "Search the Microsoft Learn corpus.",
            Domain::Learn,
        ),
        tool_kb_navigate(ctx),
    ]
}

#[derive(Debug, Deserialize)]
struct KbNavigateArgs {
    document_id: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default = "default_navigate_depth")]
    depth: u32,
}
fn default_navigate_depth() -> u32 {
    1
}

fn tool_kb_navigate(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: KbNavigateArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.kb.navigate: invalid arguments: {e}"
                    )))
                }
            };
            let store = ctx.rag.store();
            let tree = match store.get_document_tree(&args.document_id).await {
                Ok(Some(t)) => t,
                Ok(None) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.kb.navigate: document '{}' not found",
                        args.document_id
                    )))
                }
                Err(e) => return Ok(CallToolResult::error(format!("d365.kb.navigate: {e}"))),
            };
            let path = args.path.as_deref().unwrap_or("");
            let node = if path.is_empty() {
                &tree.root
            } else {
                match tree.root.find(path) {
                    Some(n) => n,
                    None => return Ok(CallToolResult::error(format!(
                        "d365.kb.navigate: path '{path}' not found in document tree (max_depth={}, leaf_count={})",
                        tree.max_depth, tree.leaf_count,
                    ))),
                }
            };
            let view = serialize_node_bounded(node, args.depth);
            render_json(
                "d365.kb.navigate",
                &serde_json::json!({
                    "document_id": tree.document_id,
                    "max_depth": tree.max_depth,
                    "leaf_count": tree.leaf_count,
                    "node": view,
                }),
            )
        }
    });
    ToolDescriptor::new(
        "d365.kb.navigate",
        Some("Walk the hierarchical document tree section by section. Pass a document_id and an optional dotted path (e.g. '1.2.1') and depth to bound the returned subtree. Use this for long Microsoft Learn pages / X++ source files when similarity-blind retrieval would miss the right section.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "document_id": {"type": "string", "description": "Document id, e.g. 'learn:finance/period-close'"},
                "path": {"type": "string", "description": "Optional dotted path, e.g. '1.2'. Omit to start at the root."},
                "depth": {"type": "integer", "minimum": 0, "maximum": 4, "default": 1}
            },
            "required": ["document_id"],
            "additionalProperties": false
        })),
        Arc::new(handler),
    )
}

fn serialize_node_bounded(node: &d365_automate_kb::DocTreeNode, depth: u32) -> serde_json::Value {
    let children: Vec<serde_json::Value> = if depth == 0 {
        node.children
            .iter()
            .map(|c| {
                serde_json::json!({
                    "path": c.path, "title": c.title, "summary": c.summary,
                    "approx_tokens": c.approx_tokens, "child_count": c.children.len(),
                })
            })
            .collect()
    } else {
        node.children
            .iter()
            .map(|c| serialize_node_bounded(c, depth - 1))
            .collect()
    };
    serde_json::json!({
        "path": node.path, "depth": node.depth, "title": node.title, "summary": node.summary,
        "start_index": node.start_index, "end_index": node.end_index,
        "approx_tokens": node.approx_tokens, "children": children,
    })
}

#[derive(Debug, Deserialize)]
struct RagSearchArgs {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
}
fn default_top_k() -> usize {
    5
}

fn rag_search_schema() -> ToolInputSchema {
    ToolInputSchema::from_value(serde_json::json!({
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Free-text query"},
            "top_k": {"type": "integer", "minimum": 1, "maximum": 50, "default": 5}
        },
        "required": ["query"],
        "additionalProperties": false
    }))
}

fn make_rag_tool(
    ctx: &Arc<ServerContext>,
    name: &str,
    description: &str,
    domain: Domain,
) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let tool_name = name.to_string();
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        let tool_name = tool_name.clone();
        async move {
            let args: RagSearchArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "{tool_name}: invalid arguments: {e}"
                    )))
                }
            };
            let q_vec = ctx
                .embedder
                .embed(std::slice::from_ref(&args.query))
                .await
                .ok()
                .and_then(|mut v| v.pop());
            let hits = ctx
                .rag
                .search(Query {
                    text: &args.query,
                    domain: Some(domain),
                    top_k: args.top_k,
                    embedding: q_vec,
                })
                .await;
            match hits {
                Err(e) => Ok(CallToolResult::error(format!("{tool_name}: {e}"))),
                Ok(hits) if hits.is_empty() => Ok(CallToolResult::text(format!(
                    "{tool_name}: no matches for \"{}\"",
                    args.query
                ))),
                Ok(hits) => {
                    let mut lines = vec![format!(
                        "{tool_name}: {} hit(s) for \"{}\"",
                        hits.len(),
                        args.query
                    )];
                    for h in &hits {
                        lines.push(format!(
                            "- [{:?}] {} ({:.3}) — {}\n  uri: {}",
                            h.layer,
                            h.hit.chunk.title,
                            h.hit.score,
                            truncate(&h.hit.chunk.text, 160),
                            h.hit.chunk.uri,
                        ));
                    }
                    Ok(CallToolResult {
                        content: vec![ToolContent::text(lines.join("\n"))],
                        is_error: false,
                    })
                }
            }
        }
    });
    ToolDescriptor::new(
        name,
        Some(description.into()),
        rag_search_schema(),
        Arc::new(handler),
    )
}

// ===========================================================================
// Service / environment / data-entity tools
// ===========================================================================

pub fn service_tools(ctx: &Arc<ServerContext>) -> Vec<ToolDescriptor> {
    vec![
        tool_env_info(ctx),
        tool_env_health(ctx),
        tool_cache_stats(ctx),
        tool_cache_invalidate(ctx),
        tool_service_search(ctx),
        tool_service_metadata(ctx),
        tool_service_bulk_metadata(ctx),
        tool_service_call(ctx),
        tool_entity_read(ctx),
        tool_entity_structure(ctx),
        tool_docs_search(ctx),
        tool_infolog_parse(ctx),
        tool_customer_search(ctx),
        tool_customer_get(ctx),
    ]
}

fn tool_env_info(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |_args: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            match ctx.d365_client.environment_info().await {
                Ok(info) => render_json("d365.env.info", &info),
                Err(e) => Ok(CallToolResult::error(format!("d365.env.info: {e}"))),
            }
        }
    });
    ToolDescriptor::new("d365.env.info",
        Some("Dynamics 365 environment identity: environment, default legal entity, version, base URL. Always read-only.".into()),
        ToolInputSchema::from_value(serde_json::json!({"type": "object", "additionalProperties": false})),
        Arc::new(handler))
}

fn tool_env_health(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |_args: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let pool = ctx.d365_client.pool_status();
            let snap = serde_json::json!({
                "pool": { "cap": pool.cap, "available": pool.available, "in_use": pool.cap - pool.available },
                "read_only_mode": ctx.read_only,
                "connection": ctx.meta_client.connection().redacted(),
                "graph": {
                    "nodes": ctx.graph.graph.stats().node_count,
                    "edges": ctx.graph.graph.stats().edge_count,
                    "communities": ctx.graph.communities.communities.len(),
                },
                "protocol_version": mcp_core::PROTOCOL_VERSION,
            });
            render_json("d365.env.health", &snap)
        }
    });
    ToolDescriptor::new("d365.env.health",
        Some("Operator health snapshot: connection pool, read-only mode, metadata connection summary, graph stats. Always read-only.".into()),
        ToolInputSchema::from_value(serde_json::json!({"type": "object", "additionalProperties": false})),
        Arc::new(handler))
}

fn tool_cache_stats(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |_args: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            match &ctx.metadata_cache {
                Some(cache) => {
                    let s = cache.stats().await;
                    render_json(
                        "d365.env.cache_stats",
                        &serde_json::json!({
                            "hits": s.hits, "misses": s.misses, "entries": s.entries,
                            "evictions": s.evictions, "hit_ratio": s.hit_ratio(),
                        }),
                    )
                }
                None => Ok(CallToolResult::error(
                    "d365.env.cache_stats: metadata cache disabled",
                )),
            }
        }
    });
    ToolDescriptor::new(
        "d365.env.cache_stats",
        Some("Live hit/miss counters for the service-metadata cache. JSON.".into()),
        ToolInputSchema::from_value(
            serde_json::json!({"type": "object", "additionalProperties": false}),
        ),
        Arc::new(handler),
    )
}

fn tool_cache_invalidate(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |_args: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            match &ctx.metadata_cache {
                Some(cache) => {
                    cache.invalidate_all().await;
                    Ok(CallToolResult::text("metadata cache invalidated"))
                }
                None => Ok(CallToolResult::error(
                    "d365.env.cache_invalidate: metadata cache disabled",
                )),
            }
        }
    });
    ToolDescriptor::new("d365.env.cache_invalidate",
        Some("Drop every cached service-metadata entry (e.g. after a metadata-changing deployment). Always allowed.".into()),
        ToolInputSchema::from_value(serde_json::json!({"type": "object", "additionalProperties": false})),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_limit_20")]
    limit: usize,
}
fn default_limit_20() -> usize {
    20
}

fn tool_service_search(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: SearchArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.service.search: invalid arguments: {e}"
                    )))
                }
            };
            match ctx
                .d365_client
                .search_service(&args.query, args.limit)
                .await
            {
                Ok(r) => render_json("d365.service.search", &r),
                Err(e) => Ok(CallToolResult::error(format!("d365.service.search: {e}"))),
            }
        }
    });
    ToolDescriptor::new("d365.service.search",
        Some("Search OData actions / Custom Service operations by name, description, or service group.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
            },
            "required": ["query"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct MetaArgs {
    operation: String,
    #[serde(default = "default_lang")]
    language: String,
}
fn default_lang() -> String {
    "en-us".into()
}

fn tool_service_metadata(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: MetaArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.service.metadata: invalid arguments: {e}"
                    )))
                }
            };
            match ctx
                .d365_client
                .service_metadata(&args.operation, &args.language)
                .await
            {
                Ok(m) => render_json("d365.service.metadata", &m),
                Err(e) => Ok(CallToolResult::error(format!(
                    "d365.service.metadata [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("d365.service.metadata",
        Some("Parameter signature, read-only flag, change-set requirement, and security privileges for one OData operation. Call this before d365.service.call.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {"type": "string", "description": "Operation name, e.g. LedgerGeneralJournalEntryPost"},
                "language": {"type": "string", "default": "en-us"}
            },
            "required": ["operation"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct BulkArgs {
    operations: Vec<String>,
    #[serde(default = "default_lang")]
    language: String,
}

fn tool_service_bulk_metadata(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: BulkArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.service.bulk_metadata: invalid arguments: {e}"
                    )))
                }
            };
            if args.operations.is_empty() {
                return Ok(CallToolResult::error(
                    "d365.service.bulk_metadata: operations must not be empty",
                ));
            }
            match ctx
                .d365_client
                .bulk_service_metadata(&args.operations, &args.language)
                .await
            {
                Ok(m) => render_json("d365.service.bulk_metadata", &m),
                Err(e) => Ok(CallToolResult::error(format!(
                    "d365.service.bulk_metadata: {e}"
                ))),
            }
        }
    });
    ToolDescriptor::new("d365.service.bulk_metadata",
        Some("Fetch metadata for several operations in one call; returns found operations plus a `missing` list.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "operations": {"type": "array", "items": {"type": "string"}, "minItems": 1},
                "language": {"type": "string", "default": "en-us"}
            },
            "required": ["operations"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

fn tool_service_call(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let commit = arguments
                .get("commit")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let audit_args = arguments.clone();
            let request: ServiceCallRequest = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.service.call: invalid arguments: {e}"
                    )))
                }
            };
            if commit {
                // Transactional write: submit the operation inside an atomic
                // $batch change set (gated by --enable-writes). Every attempt audited.
                let started = Instant::now();
                return match execute_write_operation(
                    ctx.d365_client.as_ref(),
                    request,
                    ctx.read_only,
                )
                .await
                {
                    Ok(outcome) => {
                        let audit_outcome = if outcome.committed {
                            AuditOutcome::ok(format!("{} committed", outcome.operation))
                        } else {
                            AuditOutcome::failed(
                                0,
                                format!(
                                    "{} not committed (rolled_back={})",
                                    outcome.operation, outcome.rolled_back
                                ),
                            )
                        };
                        record_write_audit(
                            &ctx,
                            "d365.service.call",
                            &audit_args,
                            audit_outcome,
                            started,
                        )
                        .await;
                        render_json(
                            "d365.service.call",
                            &serde_json::json!({
                                "operation": outcome.operation,
                                "committed": outcome.committed,
                                "rolled_back": outcome.rolled_back,
                                "messages": outcome.messages,
                                "result": outcome.result,
                            }),
                        )
                    }
                    Err(e) => {
                        let audit_outcome = match &e {
                            D365Error::PermissionDenied(r) => AuditOutcome::denied(r.clone()),
                            other => AuditOutcome::failed(other.code().as_i32(), other.to_string()),
                        };
                        record_write_audit(
                            &ctx,
                            "d365.service.call",
                            &audit_args,
                            audit_outcome,
                            started,
                        )
                        .await;
                        Ok(CallToolResult::error(format!(
                            "d365.service.call [{:?}]: {e}",
                            e.code()
                        )))
                    }
                };
            }
            match ctx.d365_client.call_service(request, ctx.read_only).await {
                Ok(result) => render_json("d365.service.call", &result),
                Err(e) => Ok(CallToolResult::error(format!(
                    "d365.service.call [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("d365.service.call",
        Some("Invoke an OData action / Custom Service operation by name with a parameters object. Read-only mode (default) blocks any operation not declared safe. Set commit=true to submit it inside an atomic $batch change set with automatic commit/rollback based on the returned Infolog (requires --enable-writes).".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {"type": "string", "description": "OData operation name"},
                "parameters": {"type": "object", "description": "Operation parameter object"},
                "timeout_ms": {"type": "integer", "minimum": 100, "maximum": 600000, "default": 30000},
                "require_read_only_safe": {"type": "boolean", "default": true},
                "commit": {"type": "boolean", "default": false, "description": "Submit as an atomic $batch change-set write (requires --enable-writes)."}
            },
            "required": ["operation"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

fn tool_entity_read(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let request: ReadEntityRequest = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.entity.read: invalid arguments: {e}"
                    )))
                }
            };
            match ctx.d365_client.read_entity(request).await {
                Ok(rows) => render_json(
                    "d365.entity.read",
                    &serde_json::json!({"rows": rows, "count": rows.len()}),
                ),
                Err(e) => Ok(CallToolResult::error(format!(
                    "d365.entity.read [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("d365.entity.read",
        Some("Read rows from a Dynamics 365 data entity with optional $select projection and $filter clauses. Company-scoped entities are automatically restricted to the connection's legal entity. Hard-capped at 1000 rows.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "entity": {"type": "string", "description": "Data entity name (e.g. ReleasedProductsV2, CompaniesV2)"},
                "fields": {"type": "array", "items": {"type": "string"}, "description": "$select projection; empty = all fields"},
                "filters": {"type": "array", "items": {"type": "string"}, "description": "$filter clauses, e.g. \"Status eq 'Open'\""},
                "max_rows": {"type": "integer", "minimum": 1, "maximum": MAX_ROWS_HARD_CAP, "default": 100}
            },
            "required": ["entity"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct EntityStructArgs {
    entity: String,
}

fn tool_entity_structure(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: EntityStructArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.entity.structure: invalid arguments: {e}"
                    )))
                }
            };
            match ctx.d365_client.entity_structure(&args.entity).await {
                Ok(s) => render_json("d365.entity.structure", &s),
                Err(e) => Ok(CallToolResult::error(format!(
                    "d365.entity.structure [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("d365.entity.structure",
        Some("Field metadata for a data entity: EDM types, key fields, company-scoping flag, security duty, and the SAP→D365 mapping note.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"entity": {"type": "string"}},
            "required": ["entity"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

fn tool_docs_search(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    // Hybrid retrieval across the whole corpus (no domain filter).
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: RagSearchArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.docs.search: invalid arguments: {e}"
                    )))
                }
            };
            let q_vec = ctx
                .embedder
                .embed(std::slice::from_ref(&args.query))
                .await
                .ok()
                .and_then(|mut v| v.pop());
            match ctx
                .rag
                .search(Query {
                    text: &args.query,
                    domain: None,
                    top_k: args.top_k,
                    embedding: q_vec,
                })
                .await
            {
                Err(e) => Ok(CallToolResult::error(format!("d365.docs.search: {e}"))),
                Ok(hits) if hits.is_empty() => Ok(CallToolResult::text(format!(
                    "d365.docs.search: no matches for \"{}\"",
                    args.query
                ))),
                Ok(hits) => {
                    let mut lines = vec![format!(
                        "d365.docs.search: {} hit(s) for \"{}\"",
                        hits.len(),
                        args.query
                    )];
                    for h in &hits {
                        lines.push(format!(
                            "- [{:?}] {} ({:.3}) — {}\n  uri: {}",
                            h.layer,
                            h.hit.chunk.title,
                            h.hit.score,
                            truncate(&h.hit.chunk.text, 160),
                            h.hit.chunk.uri
                        ));
                    }
                    Ok(CallToolResult {
                        content: vec![ToolContent::text(lines.join("\n"))],
                        is_error: false,
                    })
                }
            }
        }
    });
    ToolDescriptor::new("d365.docs.search",
        Some("Default hybrid retrieval (dense + BM25 + RRF + rerank) across the whole D365 corpus. Start here; promote to kb.multi_hop only for dependency / impact questions.".into()),
        rag_search_schema(), Arc::new(handler))
}

#[derive(Deserialize)]
struct InfologArgs {
    value: serde_json::Value,
}

fn tool_infolog_parse(_ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let handler = ToolFn(move |arguments: serde_json::Value| async move {
        let args: InfologArgs = match serde_json::from_value(arguments) {
            Ok(a) => a,
            Err(e) => {
                return Ok(CallToolResult::error(format!(
                    "d365.infolog.parse: invalid arguments: {e}"
                )))
            }
        };
        let messages = parse_infolog(&args.value);
        let failed = messages.iter().any(|m| m.is_failure());
        render_json(
            "d365.infolog.parse",
            &serde_json::json!({
                "messages": messages, "count": messages.len(), "has_failure": failed,
            }),
        )
    });
    ToolDescriptor::new("d365.infolog.parse",
        Some("Parse an OData error payload or an Infolog message list into typed messages with severities, and report whether any message indicates failure.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"value": {"description": "OData error payload or Infolog array"}},
            "required": ["value"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

// --- d365.customer.* (convenience wrappers over the CustomersV3 entity) ----

fn tool_customer_search(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: SearchArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.customer.search: invalid arguments: {e}"
                    )))
                }
            };
            let req = ReadEntityRequest {
                entity: "CustomersV3".into(),
                fields: vec![],
                filters: vec![format!("OrganizationName like '%{}%'", args.query)],
                max_rows: args.limit.min(MAX_ROWS_HARD_CAP),
            };
            match ctx.d365_client.read_entity(req).await {
                Ok(rows) => render_json(
                    "d365.customer.search",
                    &serde_json::json!({"rows": rows, "count": rows.len()}),
                ),
                Err(e) => Ok(CallToolResult::error(format!(
                    "d365.customer.search [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new(
        "d365.customer.search",
        Some("Search customers (CustomersV3) by organization name. Read-only.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}},
            "required": ["query"],
            "additionalProperties": false
        })),
        Arc::new(handler),
    )
}

#[derive(Deserialize)]
struct CustomerGetArgs {
    customer_account: String,
}

fn tool_customer_get(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: CustomerGetArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.customer.get: invalid arguments: {e}"
                    )))
                }
            };
            let req = ReadEntityRequest {
                entity: "CustomersV3".into(),
                fields: vec![],
                filters: vec![format!("CustomerAccount eq '{}'", args.customer_account)],
                max_rows: 1,
            };
            match ctx.d365_client.read_entity(req).await {
                Ok(rows) if rows.is_empty() => Ok(CallToolResult::error(format!(
                    "d365.customer.get: customer '{}' not found",
                    args.customer_account
                ))),
                Ok(rows) => render_json("d365.customer.get", &rows[0]),
                Err(e) => Ok(CallToolResult::error(format!(
                    "d365.customer.get [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new(
        "d365.customer.get",
        Some("Fetch a single customer (CustomersV3) by account number. Read-only.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"customer_account": {"type": "string"}},
            "required": ["customer_account"],
            "additionalProperties": false
        })),
        Arc::new(handler),
    )
}

// ===========================================================================
// Metadata (X++ / AOT) tools — xpp.meta.*
// ===========================================================================

pub fn meta_tools(ctx: &Arc<ServerContext>) -> Vec<ToolDescriptor> {
    vec![
        meta_get(
            ctx,
            "xpp.meta.get_class",
            "Retrieve X++ class source.",
            MetaKind::Class,
        ),
        meta_get(
            ctx,
            "xpp.meta.get_interface",
            "Retrieve X++ interface source.",
            MetaKind::Interface,
        ),
        meta_get(
            ctx,
            "xpp.meta.get_table",
            "Retrieve X++ table definition.",
            MetaKind::Table,
        ),
        meta_get(
            ctx,
            "xpp.meta.get_job",
            "Retrieve X++ job source.",
            MetaKind::Job,
        ),
        meta_get_data_entity(ctx),
        meta_get_model_contents(ctx),
        meta_search(ctx),
        meta_cross_reference(ctx),
        meta_get_entity_contents(ctx),
        meta_deploy(ctx), // write — hidden in read-only mode by exposure policy
    ]
}

#[derive(Clone, Copy)]
enum MetaKind {
    Class,
    Interface,
    Table,
    Job,
}

#[derive(Deserialize)]
struct NameArgs {
    name: String,
}

fn name_schema() -> ToolInputSchema {
    ToolInputSchema::from_value(serde_json::json!({
        "type": "object",
        "properties": {"name": {"type": "string", "description": "X++ object name"}},
        "required": ["name"],
        "additionalProperties": false,
    }))
}

fn meta_get(ctx: &Arc<ServerContext>, name: &str, desc: &str, kind: MetaKind) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let tool_name = name.to_string();
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        let tool_name = tool_name.clone();
        async move {
            let args: NameArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => return Ok(CallToolResult::error(format!("{tool_name}: {e}"))),
            };
            let result = match kind {
                MetaKind::Class => ctx.meta_client.get_class(&args.name).await,
                MetaKind::Interface => ctx.meta_client.get_interface(&args.name).await,
                MetaKind::Table => ctx.meta_client.get_table(&args.name).await,
                MetaKind::Job => ctx.meta_client.get_job(&args.name).await,
            };
            match result {
                Ok(p) => render_json(&tool_name, &p),
                Err(e) => Ok(CallToolResult::error(format!(
                    "{tool_name} [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new(name, Some(desc.into()), name_schema(), Arc::new(handler))
}

fn meta_get_data_entity(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: NameArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "xpp.meta.get_data_entity: {e}"
                    )))
                }
            };
            match ctx.meta_client.get_data_entity(&args.name).await {
                Ok(v) => render_json("xpp.meta.get_data_entity", &v),
                Err(e) => Ok(CallToolResult::error(format!(
                    "xpp.meta.get_data_entity [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("xpp.meta.get_data_entity",
        Some("Retrieve a data entity definition (public collection name, properties, source) via the Metadata API.".into()),
        name_schema(), Arc::new(handler))
}

#[derive(Deserialize)]
struct ModelArgs {
    model: String,
}

fn meta_get_model_contents(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: ModelArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "xpp.meta.get_model_contents: {e}"
                    )))
                }
            };
            match ctx.meta_client.get_model_contents(&args.model).await {
                Ok(c) => render_json("xpp.meta.get_model_contents", &c),
                Err(e) => Ok(CallToolResult::error(format!(
                    "xpp.meta.get_model_contents [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new(
        "xpp.meta.get_model_contents",
        Some("List the objects in a model (classes, jobs, interfaces, tables, ...).".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"model": {"type": "string"}},
            "required": ["model"],
            "additionalProperties": false,
        })),
        Arc::new(handler),
    )
}

const XPP_KIND_ENUM: &[&str] = &[
    "class",
    "interface",
    "table",
    "data_entity",
    "view",
    "form",
    "job",
    "query",
    "enum_type",
    "extended_data_type",
    "macro",
    "model",
    "custom_service",
    "menu_item",
];

#[derive(Deserialize)]
struct MetaSearchArgs {
    query: String,
    #[serde(default)]
    kind: Option<XppObjectKind>,
    #[serde(default = "default_max_results")]
    max_results: usize,
}
fn default_max_results() -> usize {
    25
}

fn meta_search(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: MetaSearchArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => return Ok(CallToolResult::error(format!("xpp.meta.search: {e}"))),
            };
            let req = MetaSearchRequest {
                query: args.query,
                kind: args.kind,
                max_results: args.max_results,
            };
            match ctx.meta_client.search(req).await {
                Ok(hits) => render_json("xpp.meta.search", &serde_json::json!({"hits": hits})),
                Err(e) => Ok(CallToolResult::error(format!(
                    "xpp.meta.search [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("xpp.meta.search",
        Some("Live X++/AOT object search via the Metadata API (different from xpp.search, which queries the RAG-indexed corpus). Constrained kind enum.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "kind": {"type": "string", "enum": XPP_KIND_ENUM},
                "max_results": {"type": "integer", "minimum": 1, "maximum": 100, "default": 25}
            },
            "required": ["query"],
            "additionalProperties": false,
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct CrossRefArgs {
    name: String,
    kind: XppObjectKind,
}

fn meta_cross_reference(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: CrossRefArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "xpp.meta.cross_reference: {e}"
                    )))
                }
            };
            let req = CrossReferenceRequest {
                name: args.name,
                kind: args.kind,
            };
            match ctx.meta_client.cross_reference(req).await {
                Ok(hits) => render_json(
                    "xpp.meta.cross_reference",
                    &serde_json::json!({"hits": hits}),
                ),
                Err(e) => Ok(CallToolResult::error(format!(
                    "xpp.meta.cross_reference [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("xpp.meta.cross_reference",
        Some("Impact analysis: list places that use a given X++ object (the analog of ABAP where-used). Returns object name, kind, location, and usage type (implements / call / read / write).".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "kind": {"type": "string", "enum": XPP_KIND_ENUM}
            },
            "required": ["name", "kind"],
            "additionalProperties": false,
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct EntityContentsArgs {
    entity: String,
    #[serde(default = "default_max_rows_100")]
    max_rows: usize,
}
fn default_max_rows_100() -> usize {
    100
}

fn meta_get_entity_contents(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: EntityContentsArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "xpp.meta.get_entity_contents: {e}"
                    )))
                }
            };
            match ctx
                .meta_client
                .get_entity_contents(&args.entity, args.max_rows)
                .await
            {
                Ok(rows) => render_json(
                    "xpp.meta.get_entity_contents",
                    &serde_json::json!({"rows": rows, "count": rows.len()}),
                ),
                Err(e) => Ok(CallToolResult::error(format!(
                    "xpp.meta.get_entity_contents [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("xpp.meta.get_entity_contents",
        Some("Data-entity contents via the Metadata path. Some environments block bulk entity reads — error code EntityDataBlocked tells the agent to fall back to d365.entity.read (OData).".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "entity": {"type": "string"},
                "max_rows": {"type": "integer", "minimum": 1, "maximum": 1000, "default": 100}
            },
            "required": ["entity"],
            "additionalProperties": false,
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct DeployArgs {
    name: String,
    kind: XppObjectKind,
}

fn meta_deploy(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: DeployArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => return Ok(CallToolResult::error(format!("xpp.meta.deploy: {e}"))),
            };
            let req = DeployRequest {
                name: args.name,
                kind: args.kind,
            };
            let call_ctx = MetaCallContext {
                read_only: ctx.read_only,
            };
            match ctx.meta_client.deploy(req, call_ctx).await {
                Ok(outcome) => render_json("xpp.meta.deploy", &outcome),
                Err(e) => Ok(CallToolResult::error(format!(
                    "xpp.meta.deploy [{:?}]: {e}",
                    e.code()
                ))),
            }
        }
    });
    ToolDescriptor::new("xpp.meta.deploy",
        Some("Build & deploy an X++ object (state-mutating; the analog of ABAP activate). Hidden in read-only mode by the server exposure policy; still re-checks the per-request read-only flag.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "kind": {"type": "string", "enum": XPP_KIND_ENUM}
            },
            "required": ["name", "kind"],
            "additionalProperties": false,
        })),
        Arc::new(handler))
        .with_writes()
}

// ===========================================================================
// Graph tools (GraphRAG L3, HippoRAG L4, RAPTOR L5)
// ===========================================================================

pub fn graph_tools(ctx: &Arc<ServerContext>) -> Vec<ToolDescriptor> {
    vec![
        kb_multi_hop(ctx),
        kb_global_query(ctx),
        kb_summarise(ctx),
        kb_graph_neighborhood(ctx),
    ]
}

#[derive(Deserialize)]
struct MultiHopArgs {
    query: String,
    #[serde(default = "default_max_hops")]
    max_hops: u32,
    #[serde(default = "default_top_k_graph")]
    top_k: usize,
    #[serde(default = "default_max_seeds")]
    max_seeds: usize,
}
fn default_max_hops() -> u32 {
    4
}
fn default_top_k_graph() -> usize {
    8
}
fn default_max_seeds() -> usize {
    3
}

fn kb_multi_hop(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: MultiHopArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "kb.multi_hop: invalid arguments: {e}"
                    )))
                }
            };
            let response =
                ctx.graph
                    .multi_hop(&args.query, args.max_hops, args.top_k, args.max_seeds);
            render_json("kb.multi_hop", &response)
        }
    });
    ToolDescriptor::new("kb.multi_hop",
        Some("HippoRAG-style multi-hop traversal (Personalised PageRank) across the Dynamics 365 knowledge graph. Use this for impact / cross-reference / dependency-chain queries. Returns nodes with hop distance from any seed.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "max_hops": {"type": "integer", "minimum": 1, "maximum": 6, "default": 4},
                "top_k": {"type": "integer", "minimum": 1, "maximum": 50, "default": 8},
                "max_seeds": {"type": "integer", "minimum": 1, "maximum": 10, "default": 3}
            },
            "required": ["query"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct GlobalQueryArgs {
    query: String,
    #[serde(default = "default_top_k_3")]
    top_k: usize,
}
fn default_top_k_3() -> usize {
    3
}

fn kb_global_query(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: GlobalQueryArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "kb.global_query: invalid arguments: {e}"
                    )))
                }
            };
            let response = ctx.graph.community_query(&args.query, args.top_k);
            render_json("kb.global_query", &response)
        }
    });
    ToolDescriptor::new("kb.global_query",
        Some("GraphRAG community-level Q&A. Returns the top communities (clusters of related entities) overlapping the query, with members and a synthesised summary. Use this for global / analytical / cross-domain questions.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"query": {"type": "string"}, "top_k": {"type": "integer", "minimum": 1, "maximum": 10, "default": 3}},
            "required": ["query"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct SummariseArgs {
    #[serde(default = "default_level_2")]
    level: u32,
    #[serde(default = "default_top_k_10")]
    top_k: usize,
}
fn default_level_2() -> u32 {
    2
}
fn default_top_k_10() -> usize {
    10
}

fn kb_summarise(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: SummariseArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "kb.summarise: invalid arguments: {e}"
                    )))
                }
            };
            let response = ctx.graph.raptor_summary(args.level, args.top_k);
            render_json("kb.summarise", &response)
        }
    });
    ToolDescriptor::new("kb.summarise",
        Some("RAPTOR hierarchical summary at the requested level (0 = leaves, 1 = communities, 2 = module roll-ups). Use for granularity-aware orientation queries.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"level": {"type": "integer", "minimum": 0, "maximum": 2, "default": 2}, "top_k": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10}},
            "additionalProperties": false
        })),
        Arc::new(handler))
}

#[derive(Deserialize)]
struct NeighborhoodArgs {
    seeds: Vec<String>,
    #[serde(default = "default_max_hops")]
    max_hops: u32,
    #[serde(default = "default_top_k_graph")]
    top_k: usize,
}

fn kb_graph_neighborhood(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let args: NeighborhoodArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "kb.graph_neighborhood: invalid arguments: {e}"
                    )))
                }
            };
            if args.seeds.is_empty() {
                return Ok(CallToolResult::error(
                    "kb.graph_neighborhood: seeds must not be empty",
                ));
            }
            let response = ctx
                .graph
                .neighborhood(&args.seeds, args.max_hops, args.top_k);
            render_json("kb.graph_neighborhood", &response)
        }
    });
    ToolDescriptor::new("kb.graph_neighborhood",
        Some("Multi-hop neighbourhood of an explicit set of entity IDs (e.g. ['xpp:GTFinPostJournal', 'svc:LedgerGeneralJournalEntry']). PPR-ranked. Use after d365.docs.search or xpp.meta.cross_reference has identified concrete entities.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "seeds": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 20},
                "max_hops": {"type": "integer", "minimum": 1, "maximum": 6, "default": 4},
                "top_k": {"type": "integer", "minimum": 1, "maximum": 50, "default": 8}
            },
            "required": ["seeds"],
            "additionalProperties": false
        })),
        Arc::new(handler))
}

// ===========================================================================
// Workflow tools — MCP 2025-06-18 elicitation, gated writes
// ===========================================================================

pub fn workflow_tools(ctx: &Arc<ServerContext>) -> Vec<ToolDescriptor> {
    vec![
        workflow_create_purchase_order(ctx),
        workflow_maintain_customer(ctx),
        workflow_deploy_package(ctx),
    ]
}

fn workflow_create_purchase_order(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let started = Instant::now();
            let audit_args = arguments.clone();
            #[derive(Deserialize)]
            struct PoArgs {
                #[serde(default)]
                vendor: Option<String>,
                #[serde(default)]
                item: Option<String>,
                #[serde(default)]
                quantity: Option<f64>,
            }
            let args: PoArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.workflow.create_purchase_order: {e}"
                    )))
                }
            };
            let schema = mcp_server::object_schema(
                serde_json::json!({
                    "vendor":        { "type": "string", "description": "Vendor account, e.g. 'V-001'", "default": args.vendor.unwrap_or_default() },
                    "item":          { "type": "string", "description": "Released product item number", "default": args.item.unwrap_or_default() },
                    "quantity":      { "type": "number", "description": "Order quantity", "default": args.quantity.unwrap_or(0.0) },
                    "legal_entity":  { "type": "string", "description": "Legal entity (DataAreaId)", "enum": ["USMF", "GBSI", "DEMF"], "default": "USMF" },
                    "currency":      { "type": "string", "description": "Document currency", "enum": ["USD", "EUR", "GBP", "JPY", "SGD"], "default": "USD" },
                    "delivery_date": { "type": "string", "description": "Requested delivery date (YYYY-MM-DD)" },
                }).as_object().unwrap().clone(),
                vec!["vendor".into(), "item".into(), "quantity".into(), "legal_entity".into(), "delivery_date".into()],
            );
            let elicit = mcp_server::elicit(
                "Confirm purchase-order details before posting. Legal entity + delivery date are mandatory.",
                schema,
            ).await;

            use mcp_core::ElicitationAction;
            match elicit.action {
                ElicitationAction::Accept => {
                    let content = elicit.content.unwrap_or_else(|| serde_json::json!({}));
                    record_write_audit(
                        &ctx,
                        "d365.workflow.create_purchase_order",
                        &audit_args,
                        AuditOutcome::ok("purchase order confirmed (mock)"),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::text(format!(
                        "Purchase order confirmed (mock; no real $batch submitted):\n\n{}\n\nNext step (when --enable-writes): d365.service.call PurchaseOrderCreate with commit=true.",
                        serde_json::to_string_pretty(&content).unwrap_or_default(),
                    )))
                }
                ElicitationAction::Decline => {
                    record_write_audit(
                        &ctx,
                        "d365.workflow.create_purchase_order",
                        &audit_args,
                        AuditOutcome::declined("user declined elicitation"),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::text(
                        "Purchase order cancelled by user (declined elicitation).",
                    ))
                }
                ElicitationAction::Cancel => {
                    record_write_audit(
                        &ctx,
                        "d365.workflow.create_purchase_order",
                        &audit_args,
                        AuditOutcome::denied("user aborted or elicitation unavailable"),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::error("Purchase order cancelled (user aborted or elicitation unavailable). No action taken."))
                }
            }
        }
    });
    ToolDescriptor::new("d365.workflow.create_purchase_order",
        Some("Guided purchase-order creation. Mid-execution the tool elicits vendor, item, quantity, legal entity, currency, and delivery date — declining the form cancels without side-effects. Wires PurchaseOrderCreate (atomic $batch) in write mode.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "vendor": {"type": "string", "description": "Initial vendor hint"},
                "item": {"type": "string", "description": "Initial item hint"},
                "quantity": {"type": "number", "description": "Initial quantity hint"}
            },
            "additionalProperties": false
        })),
        Arc::new(handler)).with_writes()
}

fn workflow_maintain_customer(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let started = Instant::now();
            let audit_args = arguments.clone();
            #[derive(Deserialize)]
            struct CmArgs {
                #[serde(default)]
                customer: Option<String>,
            }
            let args: CmArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.workflow.maintain_customer: {e}"
                    )))
                }
            };
            let pick_schema = mcp_server::object_schema(
                serde_json::json!({
                    "customer":     { "type": "string", "description": "Customer account (omit to create)", "default": args.customer.unwrap_or_default() },
                    "scope":        { "type": "string", "description": "Which view to maintain", "enum": ["general", "credit_collections", "sales_demographics"], "default": "general" },
                    "legal_entity": { "type": "string", "description": "Legal entity (DataAreaId)", "enum": ["USMF", "GBSI", "DEMF"], "default": "USMF" },
                }).as_object().unwrap().clone(),
                vec!["scope".into(), "legal_entity".into()],
            );
            let pick = mcp_server::elicit(
                "Select customer and which data view to maintain.",
                pick_schema,
            )
            .await;

            use mcp_core::ElicitationAction;
            if pick.action != ElicitationAction::Accept {
                record_write_audit(
                    &ctx,
                    "d365.workflow.maintain_customer",
                    &audit_args,
                    AuditOutcome::declined("cancelled at scope selection"),
                    started,
                )
                .await;
                return Ok(CallToolResult::error(
                    "Customer maintenance cancelled at scope selection.",
                ));
            }
            let picked = pick.content.unwrap_or(serde_json::Value::Null);
            let customer = picked
                .get("customer")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let scope = picked
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("general")
                .to_string();

            let fields_schema = match scope.as_str() {
                "general" => serde_json::json!({
                    "organization_name": {"type": "string", "description": "Customer organization name"},
                    "customer_group":    {"type": "string", "description": "Customer group"},
                    "country":           {"type": "string", "description": "Country/region (ISO-2)", "enum": ["US", "GB", "DE", "JP", "SG"]},
                }),
                "credit_collections" => serde_json::json!({
                    "credit_limit":   {"type": "number", "description": "Credit limit"},
                    "payment_terms":  {"type": "string", "description": "Terms of payment"},
                    "on_hold":        {"type": "string", "description": "Hold status", "enum": ["No", "Invoice", "All", "Payment"]},
                }),
                "sales_demographics" => serde_json::json!({
                    "sales_currency":  {"type": "string", "description": "Sales currency"},
                    "site":            {"type": "string", "description": "Default site"},
                    "mode_of_delivery":{"type": "string", "description": "Mode of delivery"},
                }),
                _ => serde_json::json!({}),
            };
            let confirm_schema =
                mcp_server::object_schema(fields_schema.as_object().unwrap().clone(), Vec::new());
            let confirm = mcp_server::elicit(
                &format!("Enter new values for customer {customer} ({scope} view). Leave fields blank to keep current values."),
                confirm_schema,
            ).await;

            match confirm.action {
                ElicitationAction::Accept => {
                    let changes = confirm.content.unwrap_or(serde_json::json!({}));
                    record_write_audit(
                        &ctx,
                        "d365.workflow.maintain_customer",
                        &audit_args,
                        AuditOutcome::ok(format!("customer {customer} change confirmed (mock)")),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::text(format!(
                        "Customer change confirmed (mock):\n  customer: {customer}\n  scope:    {scope}\n  changes:\n{}\n\nNext step (when --enable-writes): d365.service.call CustomerMaintain with commit=true.",
                        serde_json::to_string_pretty(&changes).unwrap_or_default(),
                    )))
                }
                ElicitationAction::Decline => {
                    record_write_audit(
                        &ctx,
                        "d365.workflow.maintain_customer",
                        &audit_args,
                        AuditOutcome::declined("user declined field entry"),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::text(
                        "Customer change declined; no action taken.",
                    ))
                }
                ElicitationAction::Cancel => {
                    record_write_audit(
                        &ctx,
                        "d365.workflow.maintain_customer",
                        &audit_args,
                        AuditOutcome::denied("cancelled"),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::error("Customer maintenance cancelled."))
                }
            }
        }
    });
    ToolDescriptor::new("d365.workflow.maintain_customer",
        Some("Two-step elicitation walking the user through a customer (CustomersV3) change: pick the data view, then fill in the scoped fields. Demonstrates chained elicitation — either step can be declined safely. Wires CustomerMaintain in write mode.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"customer": {"type": "string", "description": "Customer hint"}},
            "additionalProperties": false
        })),
        Arc::new(handler)).with_writes()
}

fn workflow_deploy_package(ctx: &Arc<ServerContext>) -> ToolDescriptor {
    let ctx = Arc::clone(ctx);
    let handler = ToolFn(move |arguments: serde_json::Value| {
        let ctx = Arc::clone(&ctx);
        async move {
            let started = Instant::now();
            let audit_args = arguments.clone();
            #[derive(Deserialize)]
            struct PkgArgs {
                package: Option<String>,
            }
            let args: PkgArgs = match serde_json::from_value(arguments) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(CallToolResult::error(format!(
                        "d365.workflow.deploy_package: {e}"
                    )))
                }
            };
            let initial = args.package.unwrap_or_default();
            let schema = mcp_server::object_schema(
                serde_json::json!({
                    "package":             { "type": "string", "description": "Deployable package name, e.g. 'GTFin-2026.04'", "default": initial },
                    "target_environment":  { "type": "string", "description": "Target environment", "enum": ["DEV", "UAT", "PRODUCTION"], "default": "UAT" },
                    "run_db_sync":         { "type": "boolean", "description": "Run database synchronize?", "default": true },
                    "skip_solution_checker":{ "type": "boolean", "description": "Skip Solution Checker (dangerous)", "default": false },
                    "confirmation_phrase": { "type": "string", "description": "Type the package name again to confirm" },
                }).as_object().unwrap().clone(),
                vec!["package".into(), "target_environment".into(), "confirmation_phrase".into()],
            );
            let elicit = mcp_server::elicit(
                "Package deployment is irreversible in production. Confirm details and re-enter the package name to proceed.",
                schema,
            ).await;

            use mcp_core::ElicitationAction;
            match elicit.action {
                ElicitationAction::Accept => {
                    let v = elicit.content.unwrap_or(serde_json::Value::Null);
                    let pkg = v.get("package").and_then(|x| x.as_str()).unwrap_or("");
                    let phrase = v
                        .get("confirmation_phrase")
                        .and_then(|x| x.as_str())
                        .unwrap_or("");
                    let target = v
                        .get("target_environment")
                        .and_then(|x| x.as_str())
                        .unwrap_or("UAT");
                    if pkg != phrase {
                        record_write_audit(
                            &ctx,
                            "d365.workflow.deploy_package",
                            &audit_args,
                            AuditOutcome::denied("confirmation phrase mismatch"),
                            started,
                        )
                        .await;
                        return Ok(CallToolResult::error(format!(
                            "Confirmation phrase '{phrase}' does not match package '{pkg}'. Deployment aborted.")));
                    }
                    record_write_audit(
                        &ctx,
                        "d365.workflow.deploy_package",
                        &audit_args,
                        AuditOutcome::ok(format!(
                            "package {pkg} deploy confirmed → {target} (mock)"
                        )),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::text(format!(
                        "Deployment plan confirmed (mock):\n  package:               {pkg}\n  target_environment:    {target}\n  run_db_sync:           {}\n  skip_solution_checker: {}\n\nNext step (when --enable-writes): apply the deployable package via LCS.",
                        v.get("run_db_sync").and_then(|x| x.as_bool()).unwrap_or(true),
                        v.get("skip_solution_checker").and_then(|x| x.as_bool()).unwrap_or(false),
                    )))
                }
                ElicitationAction::Decline => {
                    record_write_audit(
                        &ctx,
                        "d365.workflow.deploy_package",
                        &audit_args,
                        AuditOutcome::declined("user declined deployment"),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::text(
                        "Package deployment declined; no action taken.",
                    ))
                }
                ElicitationAction::Cancel => {
                    record_write_audit(
                        &ctx,
                        "d365.workflow.deploy_package",
                        &audit_args,
                        AuditOutcome::denied("elicitation unavailable"),
                        started,
                    )
                    .await;
                    Ok(CallToolResult::error("Package deployment cancelled (client lacks elicitation capability — refusing to proceed without confirmation)."))
                }
            }
        }
    });
    ToolDescriptor::new("d365.workflow.deploy_package",
        Some("Deploy a deployable package with a confirmation form. Requires the user to re-type the package name and explicitly opt in to dangerous flags (skip_solution_checker). Refuses entirely on clients that don't advertise the elicitation capability.".into()),
        ToolInputSchema::from_value(serde_json::json!({
            "type": "object",
            "properties": {"package": {"type": "string", "description": "Initial package hint"}},
            "additionalProperties": false
        })),
        Arc::new(handler)).with_writes()
}

// ===========================================================================
// helpers
// ===========================================================================

async fn record_write_audit(
    ctx: &ServerContext,
    tool: &str,
    arguments: &serde_json::Value,
    outcome: AuditOutcome,
    started: Instant,
) {
    ctx.audit
        .record(AuditEntry {
            event_id: AuditLog::new_event_id(),
            at_ms: AuditLog::now_ms(),
            session_id: None,
            tenant: None,
            actor: None,
            tool: tool.to_string(),
            environment: ctx.environment.clone(),
            arguments_redacted: arguments.clone(),
            outcome,
            duration_ms: started.elapsed().as_millis() as u64,
        })
        .await;
}

fn render_json<T: serde::Serialize>(tool: &str, value: &T) -> mcp_core::Result<CallToolResult> {
    match serde_json::to_string_pretty(value) {
        Ok(s) => Ok(CallToolResult {
            content: vec![ToolContent::text(s)],
            is_error: false,
        }),
        Err(e) => Ok(CallToolResult::error(format!("{tool}: serialise: {e}"))),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}
