# Roadmap

D365-Automate is a phased port of [`sap-automate`](https://github.com/rismanmattotorang/sap-automate)
onto Microsoft Dynamics 365 Finance & Operations. The full strategy, the
SAP→Dynamics 365 conceptual mapping, and the tool crosswalk live in
[`../PORTING.md`](../PORTING.md). The changelog is in
[`../CHANGELOG.md`](../CHANGELOG.md).

## Status

| Phase | Scope | State |
|---|---|---|
| 0 | Foundation & branding | ✅ Done |
| 1 | MCP protocol core | ✅ Done |
| 2 | ERP-agnostic engines + agentic layer | ✅ Done |
| 3 | Dynamics 365 backend tier (odata + meta, mock) | ✅ Done |
| 4 | MCP server: tools / resources / prompts / seed | ✅ Done |
| 5 | Apps (TUI, gateway, ingest, bench, samples) | ✅ Done |
| 6 | Web UI (Next.js) | ✅ Done |
| 7 | Deploy & CI | ✅ Done |
| 8 | Skills & docs | ✅ Done |
| 3b | Live HTTP transports (`http` feature) | ✅ Done |

All eight phases plus the live HTTP transports (3b) are complete.
`HttpD365Client` and `HttpMetadataClient` ship behind the `http` feature; the
server connects to a live environment when configured (see
[`INTEGRATION.md`](INTEGRATION.md)).

## Next

- Multi-operation atomic `$batch` change sets (today a single OData action POST
  is atomic server-side; multi-op change-set batching is the follow-up).
- Live integration tests against a developer environment, gated on `D365_*`
  secrets so CI without secrets skips cleanly.
- ONNX cross-encoder reranker slot (replace `MockReranker`).
- Real embedding provider wiring (Azure OpenAI / text-embedding-3-large).
- Power Platform connector parity (Dataverse solutions, Power Automate flows).
- Synapse Link / Microsoft Fabric analytics surfaces (see
  `skills/synapse-link-migration.md`).
