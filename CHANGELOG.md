# Changelog

All notable changes to **D365-Automate** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

D365-Automate is a port of [`sap-automate`](https://github.com/rismanmattotorang/sap-automate)
(ParagonCorp, Apache-2.0) re-targeted onto Microsoft Dynamics 365 for
GaussianTech. See [`PORTING.md`](PORTING.md) for the phased strategy.

---

## [0.9.1] — 2026-05-30 · Feature-parity pass + README

A gap analysis against [`sap-automate`](https://github.com/rismanmattotorang/sap-automate)
closed the remaining deltas so every source feature has a Dynamics 365 analog.

### Added

- **`xpp.meta.get_form`** tool — completes the X++/AOT reader set, bringing the
  tool surface to **37** (33 read-only + 4 gated writes), at parity with the
  source's 37; the `MockMetadataClient` now seeds a `GTFinJournalForm` fixture.
- **`scheduler.toml`** — Dynamics 365 monitoring jobs consumed by
  `d365-automate-gw --scheduler-config`.
- **`docs/COMPARISON.md`** and **`docs/PRODUCTION_PLAN.md`** — D365-grounded
  comparative analysis and production-readiness assessment.

### Changed

- README revised as a Gaussian Technologies deep-tech product page; resource
  count corrected to 14.

### Parity note

The only un-ported source artifacts are binary/marketing assets with no
functional role: the whitepaper PDF, the `docs/web-screens/` PNGs, and the
`docs/tui-rag-tab.html` snapshot.

---

## [0.9.0] — 2026-05-30 · Live HTTP transports (Phase 3b)

Implements the live Dynamics 365 clients behind the `http` feature; the server
now connects to a real environment when configured, falling back to the mocks.

### Added — `d365-automate-odata` (feature `http`)

- `HttpD365Client` — live F&O OData v4 client implementing `D365Client` over
  **Microsoft Entra ID** OAuth2 client-credentials (token cache + refresh).
  Entity reads build `$select` / `$filter` / `$top` with automatic `dataAreaId`
  injection and `like`→`contains` translation; unbound OData actions POST to
  `/data/<operation>`; structured HTTP-status→`D365Error` mapping. Metadata,
  search, and entity structure are served from the curated catalogue.
- `HttpD365Config` (`from_credentials` / `from_env`), secret-redacting `Debug`.

### Added — `d365-automate-meta` (feature `http`)

- `HttpMetadataClient` — live Metadata API client implementing `MetadataClient`
  over `/metadata` + `/data` with Entra auth (bearer or client-credentials).
- `load_connection` — TOML connection-file loader (search path:
  `$D365_AUTOMATE_CONNECTION_DIR`, `./.d365-automate/connections`,
  `~/.config/d365-automate/connections`). `deploy` is gated and directs to LCS.

### Changed

- `apps/d365-automate-server` selects the backend at runtime: live
  `HttpD365Client` when `D365_RESOURCE` is set, live `HttpMetadataClient` when
  `--connection` resolves to a non-mock file, otherwise the offline mocks.
- CI now exercises the `http` feature (clippy + test).

### Verified

- `http`-feature builds + **56** unit tests pass (URL/query/`$batch`/token/error
  builders, connection round-trip); clippy clean across `--all-targets`.
- Live backend confirmed to activate via env (Entra OAuth2; secret redacted).

---

## [0.8.0] — 2026-05-30 · Skills & docs (Phase 8)

Rewrites the 13 agentic skills for Dynamics 365 and ports the docs.

### Added — `skills/` (13 markdown skills, each an MCP prompt)

- `period-close-investigation` (ledger close), `xpp-code-review`,
  `extension-audit` (over-layering vs extensions), `package-deploy-elicit`,
  `data-entity-design`, `deploy-impact-analysis`, `synapse-link-migration`
  (analytics → Synapse Link / Fabric), `custom-service-scaffolding`,
  `security-sod-audit`, `po-creation-elicit`, `customer-master-elicit`,
  `aipnv-ai-pairing`, `karpathy-guidelines`. All named `d365.skill.*` and wired
  to the D365 tool surface (`d365.*`, `xpp.meta.*`, `kb.*`).
- `apps/d365-automate-server/src/main.rs` now scans `./skills`,
  `./.d365-automate/skills`, and `~/.config/d365-automate/skills`, exposing each
  as an MCP prompt.

### Added — `docs/`

- `D365_CORRECTNESS.md` (the 7 invariants + service/entity catalogue),
  `INTEGRATION.md` (Entra app registration, env vars, connection file, the
  one-site live-client swap), `ROADMAP.md`, `RUNBOOK_DEV_ENVIRONMENT.md`.

### Verified

- The server logs `loaded agentic skills skills=13`; `sample-client --list`
  shows **16 prompts** (13 `d365.skill.*` + 3 built-ins).

---

## [0.7.0] — 2026-05-30 · Deploy & CI (Phase 7)

Ports the deployment manifests and CI/release pipelines, rebranded for
Dynamics 365 / GaussianTech.

### Added — `deploy/`

- `Dockerfile` — multi-stage build on `rust:slim` → distroless/nonroot;
  builds `d365-automate-server` + `d365-automate-gw`.
- `k8s/` — namespace, ConfigMap (production AGENTS.md + `D365_AUTOMATE_*`
  env), Entra-credentials Secret template (`D365_RESOURCE` / `D365_TENANT_ID`
  / `D365_CLIENT_ID` / `D365_CLIENT_SECRET` / `D365_LEGAL_ENTITY`), 3-replica
  Deployment (read-only root FS, dropped caps), ClientIP-affinity Service,
  latency HPA (3–12), default-deny NetworkPolicy (egress restricted to
  HTTPS / Entra), PodDisruptionBudget, Kustomization. Image
  `ghcr.io/gaussiantech/d365-automate`.
- `grafana/d365-automate-overview.json` — metrics rebranded
  (`d365_service_calls_total`, `d365_pool_in_use`, `d365_authz_denied_total`).
- `d365-automate-connection.example.toml` — live-connection template.

### Added — `.github/workflows/`

- `ci.yml` — fmt, clippy (`--all-targets`), test matrix (stable/beta), a
  **Dynamics 365 correctness invariants** job running the 7 odata precision
  tests, the bench acceptance gate (`d365-automate-bench --graph`),
  cargo-audit, Docker build, kubeconform manifest lint, and the Next.js build.
- `release.yml` — multi-arch (x86_64 + aarch64) binaries, GHCR image push,
  and a GitHub Release.

### Verified

- All k8s YAML, both workflows, and the Grafana JSON parse cleanly.
- The exact Docker build command (`cargo build --release --bin
  d365-automate-server --bin d365-automate-gw`) compiles; image references are
  consistent across kustomization + deployment.

---

## [0.6.0] — 2026-05-30 · Web UI (Phase 6)

Ports the Next.js 14 operator console (`apps/web`) and re-grounds it in D365.

### Added — `apps/web`

- Six routes: Operations, Query Lab, Graph Lab, Tool Explorer, Skill Lab,
  Resources; the `/api/mcp` same-origin proxy targets the D365 MCP server.
- Re-grounded demo content: tool names (`d365.*` / `xpp.meta.*`), Operations
  tool grouping + synthetic traffic + service-metadata cache panel, Query Lab
  domain selector (`learn`/`xpp`/`flow`/`solution`) + example queries + URI
  colour map, Graph Lab entity-kind colours + examples, Skill Lab copy.

### Verified

- `npm install` + `npx next build` succeed — all 9 build outputs compile and
  TypeScript type-checks pass.

---

## [0.5.0] — 2026-05-30 · Apps (Phase 5)

Ports the remaining binaries and re-grounds their demo content in Dynamics 365.

### Added — apps

- `d365-automate-tui` — Ratatui operator console (live latency budget, service
  metadata cache panel, synthetic traffic re-grounded to the D365 tool surface).
- `d365-automate-gw` — multi-channel gateway: spawns `d365-automate-server`,
  routes channel-initiated queries to MCP tools/skills via a keyword router
  (`match_skill` re-grounded to D365 skills), 4-tier memory, scheduler driver.
- `d365-automate-ingest` (bin `d365-automate-ingest`) — crawl/chunk/embed/upsert
  CLI over `LearnCrawler` (Memory or Qdrant backend, Mock or OpenAI embedder).
- `d365-automate-bench` — acceptance harness; D365 RAG + graph workloads and the
  `./docs/sample-learn-corpus`.
- `sample-server` / `sample-client` — protocol smoke-test binaries; the client
  spawns `d365-automate-server` by default.

### Verified

- All six binaries build; `cargo clippy --all-targets` — no warnings.
- `d365-automate-bench --graph` passes both gates (RAG P95 0.074 ms ≤ 80 ms;
  graph multi-hop P95 0.084 ms ≤ 400 ms) against the seeded Learn corpus.
- `sample-client` drives the server and lists the D365 tool surface.
- `cargo test --all-features` — **137 passing**.

---

## [0.4.0] — 2026-05-30 · MCP server (Phase 4)

Wires the backend tier into a runnable MCP server: `apps/d365-automate-server`.
Backends are held behind trait objects and default to the offline mocks, so a
production swap is a one-site change — no tool/resource/prompt edits.

### Added — `apps/d365-automate-server`

- **Tools** (re-mapped per the crosswalk):
  - RAG: `xpp.search`, `flow.find_process`, `app.search_solutions`,
    `d365.learn.search`, `d365.kb.navigate`.
  - Service/entity: `d365.env.{info,health,cache_stats,cache_invalidate}`,
    `d365.service.{search,metadata,bulk_metadata,call}`,
    `d365.entity.{read,structure}`, `d365.docs.search`, `d365.infolog.parse`,
    `d365.customer.{search,get}`.
  - Metadata: `xpp.meta.{get_class,get_interface,get_table,get_job,
    get_data_entity,get_model_contents,search,cross_reference,
    get_entity_contents,deploy}`.
  - Graph: `kb.{multi_hop,global_query,summarise,graph_neighborhood}`.
  - Workflows (gated writes, elicitation): `d365.workflow.{create_purchase_order,
    maintain_customer,deploy_package}`.
- **Resources**: `d365-env://info`, `d365-entity://{name}/structure`,
  `d365-service://{name}`, `d365-meta://info`, `d365-cache://stats`,
  `agents://guardrails`.
- **Prompts**: `d365.review-service-call`, `d365.deploy-impact-analysis`,
  `xpp.review-cross-reference`, plus disk-loaded skills.
- Read-only-by-default exposure policy (`ExposurePolicy::ReadOnlyOnly`),
  `--enable-writes`, atomic `$batch` commit path on `d365.service.call`,
  audit logging, `AGENTS.md` loader, completion providers, and the
  Dynamics 365 seed corpus.
- stdio + HTTP transports (`/health`, `/metrics`, `/mcp`), Entra-credential
  resolution, and a swappable backend seam documented in `lib.rs`.

### Verified

- `cargo build --release` — binary runs; HTTP `/health` → `ok`, `initialize`
  returns capabilities + guardrails.
- `cargo clippy --all-features` — no warnings.
- `cargo test --all-features` — **137 passing** (130 + 7 server integration).

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
