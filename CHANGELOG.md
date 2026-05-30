# Changelog

All notable changes to **D365-Automate** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

D365-Automate is a port of [`sap-automate`](https://github.com/rismanmattotorang/sap-automate)
(ParagonCorp, Apache-2.0) re-targeted onto Microsoft Dynamics 365 for
GaussianTech. See [`PORTING.md`](PORTING.md) for the phased strategy.

---

## [0.2.0] — 2026-05-30 · ERP-agnostic engines + agentic layer (Phase 2)

Ports the ten ERP-agnostic engine and agentic crates and re-grounds their
domain surfaces in Dynamics 365 canon. No live backend yet — that is Phase 3.

### Added — `d365-automate-*` crates

- `d365-automate-kb` (knowledge base + hierarchical document tree),
  `d365-automate-rag` (dense + BM25 + RRF + reranker),
  `d365-automate-graph` (GraphRAG / HippoRAG / RAPTOR),
  `d365-automate-ingest` (crawler + chunker + embedder pipeline),
  `d365-automate-memory`, `d365-automate-scheduler`, `d365-automate-channels`,
  `d365-automate-skills`, `d365-automate-connectors`,
  `d365-automate-observability`.

### Changed — re-grounded SAP domain surfaces in Dynamics 365

- **Graph types** — `EntityKind`: `AbapObject`→`XppObject`, `Table`→`DataEntity`,
  `Rfc`→`Service`, `BpmnProcess`→`Flow`, `LeanixApp`→`Solution`,
  `HelpPage`→`LearnPage`; `EdgeKind`: `ReadsTable`→`ReadsEntity`,
  `WritesTable`→`WritesEntity`, `Includes`→`Uses`.
- **Seed knowledge graph** — the SAP FI journal-posting fixture re-cast as a
  Dynamics 365 GL posting fixture (`GTFinPostJournal` X++ job →
  `LedgerGeneralJournalEntry` OData service → `LedgerJournalTrans` /
  `GeneralJournalAccountEntry` entities → `FIN-CORE` solution).
- **KB `Domain`** — `SapHelp`→`Learn`, `Abap`→`Xpp`, `Bpmn`→`Flow`,
  `Leanix`→`Solution` (collection names follow).
- **Ingest** — `HelpPortalCrawler`→`LearnCrawler`,
  `parse_help_portal_html`→`parse_learn_html`, `help.sap.com`→
  `learn.microsoft.com`; demo corpus re-grounded in Dynamics 365 Finance.
- **Connectors** — `AbapConnector`→`XppConnector`, `BpmnConnector`→
  `FlowConnector`, `LeanixConnector`→`SolutionConnector`.
- **Observability** — audit field `sap_system`→`environment`; metrics
  `sap_rfc_calls_total`→`d365_service_calls_total`,
  `sap_pool_in_use`→`d365_pool_in_use`,
  `sap_authz_denied_total`→`d365_authz_denied_total`.

### Verified

- `cargo build --all-features` — clean.
- `cargo clippy --all-features` — no warnings.
- `cargo test --all-features` — **86 passing** (18 MCP + 68 engine/agentic).

---

## [0.1.0] — 2026-05-30 · Foundation & MCP protocol core (Phases 0–1)

First commit of the port. Establishes the workspace and ports the ERP-agnostic
Model Context Protocol foundation verbatim, with all SAP-specific references
re-branded for Dynamics 365 / GaussianTech.

### Added

- **Workspace skeleton** — `Cargo.toml` (resolver 2), `rust-toolchain.toml`
  (stable + rustfmt + clippy), `.gitignore`, Apache-2.0 `LICENSE`.
- **Porting strategy** — `PORTING.md`: source inventory, full SAP S/4HANA →
  Dynamics 365 conceptual mapping, naming conventions, correctness invariants,
  the 9-phase plan, and the 37-tool crosswalk.
- **Project docs** — D365/GaussianTech `README.md`, `AGENTS.md` guardrails,
  this `CHANGELOG.md`.
- **MCP protocol core (Phase 1)** — ported `mcp-core` (JSON-RPC 2.0 codec +
  MCP 2025-06-18 protocol types), `mcp-transport` (stdio + HTTP/SSE),
  `mcp-server` (capability router, tool registry, dispatch loop, elicitation),
  and `mcp-client` (request/response correlation). The few `sap` doc-comment
  and env-var references re-branded (`SAP_AUTOMATE_DISABLE_LOGGING_CAP` →
  `D365_AUTOMATE_DISABLE_LOGGING_CAP`).

### Verified

- `cargo build --all-features` — clean.
- `cargo test --all-features` — **18 passing** (3 `mcp-core`, 6 `mcp-server`
  integration, 9 `mcp-transport`).

### Pending (see `PORTING.md`)

Phases 2–8: ERP-agnostic engines, the Dynamics 365 backend tier
(`d365-automate-odata`, `d365-automate-meta`), the MCP server tool surface,
apps, web UI, deploy/CI, and skills.
