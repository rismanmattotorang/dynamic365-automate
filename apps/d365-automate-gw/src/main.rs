//! Multi-channel gateway binary.
//!
//! Acceptance gate: a Teams-initiated `d365.docs.search`
//! query returns a cited answer within the existing P95 budget plus a
//! 50 ms gateway tax.
//!
//! This binary:
//!   1. Spawns the D365-Automate MCP server as a child process over stdio.
//!   2. Registers channel adapters (CLI now; Teams / Slack / Telegram
//!      land via the trait in `d365-automate-channels`).
//!   3. Reads a sequence of `IncomingMessage` events from the configured
//!      channel(s), routes the user's intent to an MCP tool call, and
//!      writes the result back as `OutgoingMessage`.
//!   4. Owns the four-tier `MemoryManager` so the agent keeps context
//!      between turns of the same conversation.
//!   5. Drives the proactive `Scheduler` if a `scheduler.toml` is found.

use clap::Parser;
use d365_automate_channels::{
    now_ms, ChannelKind, ChannelRegistry, CliChannel, IncomingMessage, OutgoingMessage,
};
use d365_automate_memory::{MemoryEntry, MemoryManager, Tier};
use d365_automate_scheduler::{JobExecutor, ScheduledJob, Scheduler};
use mcp_client::{Client, DeclineAll};
use mcp_core::{ClientCapabilities, Implementation};
use mcp_transport::stdio::StdioTransport;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "d365-automate-gw",
    about = "Multi-channel gateway routing channel events into MCP tool calls."
)]
struct Cli {
    /// Path to the D365-Automate MCP server binary.
    #[arg(long, default_value = "target/release/d365-automate-server")]
    server: String,
    /// Optional scheduler configuration (TOML).
    #[arg(long)]
    scheduler_config: Option<String>,
    /// Simulated incoming message text from a CLI user.
    #[arg(long)]
    simulate_query: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // 1. Spawn MCP server child.
    let mut child = Command::new(&cli.server)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn '{}': {e}", cli.server))?;
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let transport = StdioTransport::new(stdout, stdin);
    let mcp = Client::spawn_stdio(transport, Arc::new(DeclineAll));

    // 2. Initialise.
    let init = mcp
        .initialize(
            Implementation {
                name: "d365-automate-gw".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            ClientCapabilities::default(),
        )
        .await?;
    tracing::info!(
        server = init.server_info.name,
        "gateway connected to MCP server"
    );

    // 3. Wire memory + channels.
    let memory = Arc::new(MemoryManager::new());
    let mut registry = ChannelRegistry::new();
    let cli_channel = CliChannel::new();
    registry.register(cli_channel.clone());
    let registry = Arc::new(registry);

    // 4. Optional scheduler.
    if let Some(path) = cli.scheduler_config.as_deref() {
        if let Ok(text) = tokio::fs::read_to_string(path).await {
            match Scheduler::parse_config(&text) {
                Ok(jobs) => {
                    let exec = Arc::new(GatewayJobExecutor {
                        mcp: mcp.clone(),
                        registry: registry.clone(),
                    });
                    let scheduler = Scheduler::new(jobs, exec);
                    tracing::info!(jobs = scheduler.jobs().len(), "scheduler loaded");
                    // Phase 8 demo: fire all jobs once, immediately.
                    let reports = scheduler.fire_all_now().await;
                    for r in reports {
                        println!(
                            "[scheduler] {} -> {} ({} ms)",
                            r.job,
                            if r.ok { "ok" } else { "ERR" },
                            r.duration_ms
                        );
                    }
                }
                Err(e) => tracing::warn!(error = %e, "scheduler config parse failed"),
            }
        }
    }

    // 5. Simulated message (paper §X-K acceptance demo).
    if let Some(query) = cli.simulate_query.as_deref() {
        let msg = IncomingMessage {
            channel: ChannelKind::Cli,
            user_id: "demo@example.com".into(),
            conversation_id: "default".into(),
            text: query.into(),
            metadata: serde_json::Value::Null,
            received_at_ms: now_ms(),
        };
        let t0 = Instant::now();
        let outgoing = route(&mcp, &memory, &msg).await?;
        registry.send(outgoing).await?;
        let total_ms = t0.elapsed().as_millis();
        let logged = cli_channel.last().await.unwrap();
        println!(
            "\n[gateway] tax = {} ms (target ≤ 50 ms + server P95)",
            total_ms
        );
        println!(
            "[gateway] sent {} characters to {}:",
            logged.text.len(),
            logged.address
        );
        println!("------------------------------------------------------------");
        println!("{}", logged.text);
        println!("------------------------------------------------------------");
    }

    drop(mcp);
    let _ = child.wait().await;
    Ok(())
}

/// Route an incoming message to an MCP tool call.  The intent classifier
/// here is intentionally simple — Phase 8 ships a routing surface, not
/// an LLM-based orchestrator.  Production wiring registers an
/// LLM-driven router that reads the four-tier memory.
async fn route(
    mcp: &Arc<Client>,
    memory: &Arc<MemoryManager>,
    msg: &IncomingMessage,
) -> anyhow::Result<OutgoingMessage> {
    // Record the user's turn in working memory.
    memory.working.append(
        &msg.conversation_id,
        MemoryEntry::new(
            Tier::Working,
            "user_turn",
            serde_json::json!({ "channel": format!("{:?}", &msg.channel), "text": msg.text }),
        )
        .with_tenant(&msg.user_id),
    );

    // Phase 8 routing: pick a tool — OR a skill — by intent keywords.
    // The router surface is identical for the LLM-driven version we'll
    // wire in production; only the picker changes.
    //
    // Convergent insight: agents should
    // invoke *skills*, not raw tools, whenever a workflow exists. The
    // gateway honours that by routing to `prompts/get` when the user's
    // intent matches a known skill.
    let lc = msg.text.to_lowercase();
    let address = format!("{}:{}", msg.channel.scheme(), msg.conversation_id);

    if let Some((skill, args)) = match_skill(&lc) {
        match mcp.get_prompt(skill, args).await {
            Ok(p) => {
                let text_payload = p
                    .messages
                    .iter()
                    .filter_map(|m| {
                        if let mcp_core::ToolContent::Text { text } = &m.content {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                memory.working.append(
                    &msg.conversation_id,
                    MemoryEntry::new(
                        Tier::Working,
                        "agent_turn",
                        serde_json::json!({ "skill": skill, "result_chars": text_payload.len() }),
                    )
                    .with_tenant(&msg.user_id),
                );
                return Ok(OutgoingMessage {
                    address,
                    text: format!(
                        "Routed via skill `{skill}`.\n\n{text_payload}\n\nSession context: {} working entries.",
                        memory.working.recent(&msg.conversation_id, 100).len(),
                    ),
                    rich: None,
                });
            }
            // If the prompt doesn't exist (older server, or skill not loaded),
            // fall through to the tool-based router below.
            Err(e) => {
                tracing::warn!(skill, error = %e, "skill routing failed, falling back to tools")
            }
        }
    }

    let (tool, args) = if lc.contains("solution checker") || lc.contains("best practice") {
        (
            "d365.docs.search",
            serde_json::json!({
                "query": "Solution Checker best-practice findings recent",
                "top_k": 3
            }),
        )
    } else if lc.contains("impact")
        || lc.contains("cross reference")
        || lc.contains("cross-reference")
        || lc.contains("depends")
    {
        (
            "kb.multi_hop",
            serde_json::json!({
                "query": msg.text,
                "top_k": 6,
                "max_hops": 4
            }),
        )
    } else {
        (
            "d365.docs.search",
            serde_json::json!({
                "query": msg.text,
                "top_k": 3
            }),
        )
    };

    let result = mcp.call_tool(tool, Some(args)).await?;
    let text_payload = result
        .content
        .iter()
        .filter_map(|c| {
            if let mcp_core::ToolContent::Text { text } = c {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Record the agent's turn so future questions can refer back.
    memory.working.append(
        &msg.conversation_id,
        MemoryEntry::new(
            Tier::Working,
            "agent_turn",
            serde_json::json!({ "tool": tool, "result_chars": text_payload.len() }),
        )
        .with_tenant(&msg.user_id),
    );

    Ok(OutgoingMessage {
        address,
        text: format!(
            "Routed via `{tool}`.\n\n{text_payload}\n\nSession context: {} working entries.",
            memory.working.recent(&msg.conversation_id, 100).len(),
        ),
        rich: None,
    })
}

/// Map a lower-cased user message to a skill + argument map.  Returns
/// `None` when no skill matches — the caller falls back to tool routing.
/// Deliberately simple keyword match; the LLM-driven router in v1.5 will
/// replace this without touching the trait surface.
fn match_skill(lc: &str) -> Option<(&'static str, Option<serde_json::Value>)> {
    if (lc.contains("sod") || lc.contains("segregation of duties") || lc.contains("security audit"))
        && (lc.contains("audit") || lc.contains("review") || lc.contains("sod"))
    {
        // Extract a trailing token as user/role; default to wildcard.
        let target = lc.split_whitespace().last().unwrap_or("*").to_uppercase();
        return Some((
            "d365.skill.security_sod_audit",
            Some(serde_json::json!({
                "user_or_role": target,
                "scope": "user"
            })),
        ));
    }
    if lc.contains("period close")
        || lc.contains("period-close")
        || (lc.contains("closing") && lc.contains("ledger"))
    {
        return Some((
            "d365.skill.period_close_investigation",
            Some(serde_json::json!({
                "legal_entity": "USMF"
            })),
        ));
    }
    if lc.contains("code review")
        || (lc.contains("review")
            && (lc.contains("x++")
                || lc.contains("xpp")
                || lc.contains("class")
                || lc.contains("job")))
    {
        return Some((
            "d365.skill.xpp_code_review",
            Some(serde_json::json!({
                "object_name": "GTFinJournalPoster",
                "kind": "class"
            })),
        ));
    }
    if lc.contains("data entity")
        || (lc.contains("odata") && (lc.contains("design") || lc.contains("expose")))
    {
        return Some((
            "d365.skill.data_entity_design",
            Some(serde_json::json!({
                "entity_name": "PurchaseOrderHeadersV2"
            })),
        ));
    }
    if lc.contains("deploy")
        && (lc.contains("impact")
            || lc.contains("package")
            || lc.contains("analyse")
            || lc.contains("analyze"))
    {
        return Some((
            "d365.skill.deploy_impact_analysis",
            Some(serde_json::json!({
                "package": "GTFin-2026.04"
            })),
        ));
    }
    if lc.contains("over-layering")
        || lc.contains("over layering")
        || lc.contains("extension audit")
        || lc.contains("overlayer")
    {
        return Some((
            "d365.skill.extension_audit",
            Some(serde_json::json!({
                "model": "GTFin"
            })),
        ));
    }
    if lc.contains("guideline")
        || lc.contains("how should i think")
        || lc.contains("preflight")
        || lc.contains("pre-flight")
    {
        return Some((
            "d365.skill.aipnv_ai_pairing",
            Some(serde_json::json!({
                "task": "investigate without a clear scope"
            })),
        ));
    }
    None
}

/// JobExecutor that fires scheduled jobs against the live MCP server
/// and posts the result through the channel registry.
struct GatewayJobExecutor {
    mcp: Arc<Client>,
    registry: Arc<ChannelRegistry>,
}

#[async_trait::async_trait]
impl JobExecutor for GatewayJobExecutor {
    async fn invoke(&self, job: &ScheduledJob) -> Result<String, String> {
        let args = if job.arguments.is_null() {
            None
        } else {
            Some(job.arguments.clone())
        };
        let result = self
            .mcp
            .call_tool(&job.tool, args)
            .await
            .map_err(|e| format!("MCP error: {e}"))?;
        let summary = result
            .content
            .iter()
            .filter_map(|c| {
                if let mcp_core::ToolContent::Text { text } = c {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        // Best-effort post to the declared channel.
        if let Some(addr) = &job.channel {
            let _ = self
                .registry
                .send(OutgoingMessage::text(addr.clone(), summary.clone()))
                .await;
        }
        Ok(format!("[{}] {} chars", job.name, summary.len()))
    }
}
