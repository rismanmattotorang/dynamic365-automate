//! End-to-end smoke tests for the D365-Automate MCP server.
//!
//! Drives the in-process `build_test_server` over a duplex transport with a
//! real MCP client: verifies tool exposure, a company-scoped entity read,
//! service metadata, Infolog parsing, and the read-only write gate.

use d365_automate_server_lib::{build_test_server, TestServerOptions};
use mcp_client::Client;
use mcp_core::{CallToolResult, ClientCapabilities, Implementation};
use mcp_transport::stdio::StdioTransport;
use std::sync::Arc;

async fn connect(read_only: bool) -> Arc<Client> {
    let (server, _ctx) = build_test_server(TestServerOptions {
        read_only,
        seed_kb: true,
        ..Default::default()
    })
    .await;

    let (s_rx, c_tx) = tokio::io::duplex(8192);
    let (c_rx, s_tx) = tokio::io::duplex(8192);
    let server_transport = StdioTransport::new(s_rx, s_tx);
    tokio::spawn(async move {
        let _ = server.run(server_transport).await;
    });

    let client_transport = StdioTransport::new(c_rx, c_tx);
    let client = Client::spawn(client_transport);
    client
        .initialize(
            Implementation {
                name: "d365-smoke".into(),
                version: "0".into(),
            },
            ClientCapabilities::default(),
        )
        .await
        .expect("initialize");
    client
}

fn extract_json(result: &CallToolResult) -> serde_json::Value {
    let text = result
        .content
        .iter()
        .find_map(|c| match c {
            mcp_core::ToolContent::Text { text } => Some(text.clone()),
            _ => None,
        })
        .expect("text content");
    serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_only_hides_write_tools_but_shows_reads() {
    let client = connect(true).await;
    let tools = client.list_tools().await.expect("list_tools");
    let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_str()).collect();
    // Read tools present.
    assert!(names.contains(&"d365.entity.read"), "have: {names:?}");
    assert!(names.contains(&"d365.service.metadata"));
    assert!(names.contains(&"xpp.meta.get_class"));
    assert!(names.contains(&"xpp.meta.get_form"));
    assert!(names.contains(&"kb.multi_hop"));
    // Write tools hidden under the read-only exposure policy.
    assert!(
        !names.contains(&"xpp.meta.deploy"),
        "deploy must be hidden in read-only mode"
    );
    assert!(!names.contains(&"d365.workflow.create_purchase_order"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_tools_appear_when_writes_enabled() {
    let client = connect(false).await;
    let tools = client.list_tools().await.expect("list_tools");
    let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"xpp.meta.deploy"));
    assert!(names.contains(&"d365.workflow.deploy_package"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_read_is_company_scoped() {
    let client = connect(true).await;
    let result = client
        .call_tool(
            "d365.entity.read",
            Some(serde_json::json!({
                "entity": "FiscalCalendarPeriod"
            })),
        )
        .await
        .expect("call");
    let body = extract_json(&result);
    let rows = body["rows"].as_array().expect("rows");
    assert!(!rows.is_empty());
    // Auto-scoped to the connection's legal entity (USMF). Rows serialize as
    // `{ "values": { ... } }`.
    assert!(rows
        .iter()
        .all(|r| r["values"]["dataAreaId"].as_str() == Some("USMF")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_metadata_reports_changeset_and_security() {
    let client = connect(true).await;
    let result = client
        .call_tool(
            "d365.service.metadata",
            Some(serde_json::json!({
                "operation": "LedgerGeneralJournalEntryPost"
            })),
        )
        .await
        .expect("call");
    let body = extract_json(&result);
    assert_eq!(body["read_only"].as_bool(), Some(false));
    assert_eq!(body["uses_changeset"].as_bool(), Some(true));
    assert!(body["security"]
        .as_array()
        .map(|a| !a.is_empty())
        .unwrap_or(false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn infolog_parse_detects_failure() {
    let client = connect(true).await;
    let result = client
        .call_tool(
            "d365.infolog.parse",
            Some(serde_json::json!({
                "value": [{"severity": "Error", "code": "X", "message": "period closed"}]
            })),
        )
        .await
        .expect("call");
    let body = extract_json(&result);
    assert_eq!(body["has_failure"].as_bool(), Some(true));
    assert_eq!(body["count"].as_u64(), Some(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_write_is_denied_in_read_only_mode() {
    let client = connect(true).await;
    // The tool exists only via call (hidden from list), but read-only mode
    // must refuse the commit path regardless.
    let result = client
        .call_tool(
            "d365.service.call",
            Some(serde_json::json!({
                "operation": "LedgerGeneralJournalEntryPost",
                "parameters": {"JournalBatchNumber": "000123", "JournalLines": []},
                "commit": true
            })),
        )
        .await
        .expect("call");
    assert!(
        result.is_error,
        "commit write must be denied in read-only mode"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resources_and_prompts_are_registered() {
    let client = connect(true).await;
    let resources = client.list_resources().await.expect("list_resources");
    let uris: Vec<&str> = resources.resources.iter().map(|r| r.uri.as_str()).collect();
    assert!(uris.contains(&"d365-env://info"), "have: {uris:?}");
    assert!(uris.iter().any(|u| u.starts_with("d365-entity://")));

    let prompts = client.list_prompts().await.expect("list_prompts");
    let pnames: Vec<&str> = prompts.prompts.iter().map(|p| p.name.as_str()).collect();
    assert!(
        pnames.contains(&"d365.review-service-call"),
        "have: {pnames:?}"
    );
    assert!(pnames.contains(&"xpp.review-cross-reference"));
}
