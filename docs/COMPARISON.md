# Comparative analysis: where D365-Automate sits

D365-Automate is an agentic runtime for Microsoft Dynamics 365 Finance &
Operations. This document records what it is measured against and the design
decisions that follow.

## Provenance

D365-Automate is a faithful port of the open-source
[`sap-automate`](https://github.com/rismanmattotorang/sap-automate)
architecture — an MCP-native agentic OS originally built for SAP S/4HANA —
re-grounded onto Dynamics 365. The MCP protocol core, the RAG/graph retrieval
engines, and the agentic gateway carry over almost verbatim; the ERP backend
tier (RFC/BAPI/DDIC/ADT) was rewritten against the Dynamics 365 backend tier
(OData v4 / Custom Services / Data Entities / Metadata API). The phase-by-phase
crosswalk lives in [`../PORTING.md`](../PORTING.md).

## The landscape it competes in

| Approach | Typical shape | Where it falls short for agents |
|---|---|---|
| **Dataverse / Power Platform connectors** | REST/OData wrappers, low-code | No agent-grade safety model (read-only-by-default, atomic write gating, elicitation); no grounding/retrieval; per-flow, not a tool surface |
| **Generic OData "proxy" MCP servers** | Config-driven OData → MCP | No curated metadata catalogue, so no stable safety annotations; no `DataAreaId` scoping; no knowledge graph |
| **Cloud-hosted copilots** | Vendor SaaS | Data leaves the tenant; closed; no on-prem / sovereign option; opaque latency |
| **D365-Automate** | Single Rust binary, MCP 2025-06-18 | Read-only by default, atomic `$batch` writes, elicitation gates, `DataAreaId` scoping, sub-ms hybrid + graph retrieval, Entra-native, on-prem capable |

## Design moves (carried from the reference architecture)

- **Read-only by default + exposure policy.** Write tools are hidden from
  `tools/list` until the operator opts in (`--enable-writes`); the per-call
  `read_only` flag is defence-in-depth.
- **Curated metadata catalogue.** Service/entity metadata and the security
  model come from a catalogue with correctness invariants enforced in CI — so
  the live client's safety annotations never drift with the wire schema.
- **Atomic unit of work.** Writes stage into an OData `$batch` change set;
  the orchestration never auto-commits and is fail-closed on unconfirmed
  outcomes (the Dynamics 365 analog of "no BAPI auto-commit").
- **Metadata cache decorator.** A TTL cache over the client trait turns
  repeated metadata reads from network round-trips into in-memory hits.
- **Hierarchical retrieval.** Hybrid RAG (dense + BM25 + RRF + rerank) for the
  default path; a typed cross-domain knowledge graph (GraphRAG / HippoRAG /
  RAPTOR) for impact and dependency questions.
- **Skills over raw tools.** Declarative workflow templates (`./skills/*.md`)
  auto-load as MCP prompts so agents invoke vetted workflows.
- **MCP 2025-06-18 utilities.** Capability negotiation, `logging/setLevel`,
  `completion/complete`, and elicitation round-trips; HTTP transport with
  Origin validation and bearer auth.

## What is deliberately different from the SAP original

| SAP-Automate | D365-Automate |
|---|---|
| RFC / BAPI (SOAP) | OData v4 actions / Custom Services |
| `BAPI_TRANSACTION_COMMIT` | atomic `$batch` change set |
| `BAPIRET2` | OData error / Infolog |
| DDIC tables, `MANDT` | data entities, `DataAreaId` |
| ABAP / ADT | X++ / Metadata (AOT) API |
| Transport (CTS) | deployable package (LCS) |
| XSUAA / logon ticket | Microsoft Entra ID OAuth2 |
| `MATNR` | released-product `ItemNumber` |
| `ACDOCA` | `GeneralJournalAccountEntry` |
