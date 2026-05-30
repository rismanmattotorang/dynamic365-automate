# Changelog

All notable changes to **D365-Automate** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

D365-Automate is a port of [`sap-automate`](https://github.com/rismanmattotorang/sap-automate)
(ParagonCorp, Apache-2.0) re-targeted onto Microsoft Dynamics 365 for
GaussianTech. See [`PORTING.md`](PORTING.md) for the phased strategy.

---

## [0.3.0] — 2026-05-30 · Dynamics 365 backend tier (Phase 3)

The core engineering of the port: rewrites the two SAP backend crates against
the Dynamics 365 F&O backend tier. The trait shapes are preserved so the
Phase 4 server ports with minimal churn.

### Added — `d365-automate-odata` (was `sap-automate-rfc`)

- `D365Client` trait (mirrors `SapClient`): `environment_info`, `search_service`,
  `service_metadata`, `bulk_service_metadata`, `call_service`, `read_entity`,
  `entity_structure`.
- `MockD365Client` with realistic GL / SCM / Sales fixtures (services + data
  entities), legal-entity (`DataAreaId`) scoping on reads.
- `transaction`: atomic OData **`$batch` change-set** write orchestration
  (`execute_write_operation`) — replaces SAP's `BAPI_TRANSACTION_COMMIT` two-phase
  protocol; never auto-commits, fail-closed on unconfirmed outcomes.
- `infolog`: OData error / Infolog parser (replaces the `bapiret2` parser).
- `credentials`: Microsoft Entra ID OAuth2 client-credentials providers
  (env / static / layered) — replaces SAP logon / XSUAA.
- `metadata_cache` TTL decorator, `pool`, `retry` + circuit breaker, and the
  `D365Error` taxonomy — ported from the RFC crate.
- Dynamics 365 correctness invariants (ported from `SAP_CORRECTNESS`):
  `every_write_operation_uses_changeset`, `every_write_operation_returns_operation_status`,
  `every_operation_references_a_security_privilege`,
  `every_company_scoped_entity_has_dataareaid_key`,
  `item_number_follows_released_product_convention`,
  `general_journal_account_entry_is_the_subledger_truth`,
  `legacy_tables_map_to_data_entities`.

### Added — `d365-automate-meta` (was `sap-automate-adt`)

- `MetadataClient` trait (mirrors `AdtClient`): `get_class` / `get_interface` /
  `get_table` / `get_job` / `get_form` / `get_model_contents` / `get_data_entity`,
  `search`, `cross_reference` (was `where_used`), `get_entity_contents`,
  and gated `deploy` (was `activate`).
- `MockMetadataClient` seeded with the GTFin / GTScm X++ fixtures that mirror
  the knowledge-graph seed; cross-reference wiring for impact analysis.
- `XppObjectKind` (AOT object kinds), `connection` model with Entra auth, and
  the `MetaError` taxonomy.

### Deferred to Phase 3b

- Live `HttpD365Client` (OData v4 + `$batch` over Entra OAuth2) and the live
  Metadata API client, behind the `http` feature. The mock is the default;
  Phase 4 runs against it.

### Verified

- `cargo build --all-features` — clean.
- `cargo clippy --all-features` — no warnings.
- `cargo test --all-features` — **130 passing** (86 + 44 new: 34 odata, 10 meta).

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
