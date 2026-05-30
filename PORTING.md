# Porting Strategy — `sap-automate` → `dynamic365-automate`

> **Goal.** Port [`rismanmattotorang/sap-automate`](https://github.com/rismanmattotorang/sap-automate)
> — ParagonCorp's agentic OS for **SAP S/4HANA** — into `dynamic365-automate`,
> the agentic OS for **Microsoft Dynamics 365** Finance & Operations, built for
> **GaussianTech**.
>
> The MCP protocol core, the RAG/graph retrieval engines, and the agentic
> gateway are ERP-agnostic and port almost verbatim. The value of this project
> is concentrated in faithfully re-mapping the **SAP backend tier** (RFC / BAPI
> / DDIC / ADT) onto the **Dynamics 365 backend tier** (OData v4 / Custom
> Services / Data Entities / Metadata API), and re-grounding the knowledge
> corpus, tools, prompts, and skills in Dynamics 365 canon.

---

## 1. Source inventory

| Layer | Source crate(s) | LOC | ERP coupling | Port effort |
|---|---|---:|---|---|
| MCP protocol | `mcp-core`, `mcp-transport`, `mcp-server`, `mcp-client` | ~2.8k | **None** | Rename refs only ✅ |
| Retrieval engines | `sap-automate-rag`, `sap-automate-graph` | ~1.6k | None | Rename + reseed |
| Knowledge base | `sap-automate-kb` | ~1.4k | Corpus only | Rename + reseed |
| Ingestion | `sap-automate-ingest` | ~1.4k | Crawl targets | Rename + retarget hosts |
| Agentic | `sap-automate-memory`, `-scheduler`, `-channels`, `-skills`, `-connectors` | ~1.1k | Light | Rename + remap connectors |
| Observability | `sap-automate-observability` | ~0.6k | None | Rename |
| **SAP backend** | **`sap-automate-rfc`**, **`sap-automate-adt`** | **~6.8k** | **Total** | **Rewrite for D365** |
| Server binary | `apps/sap-automate-server` (tools/resources/prompts/seed) | ~3.5k | High | Re-map 37 tools |
| Apps | `tui`, `gw`, `ingest`, `bench`, `sample-*` | ~1.9k | Light–med | Rename + rewire |
| Web UI | `apps/web` (Next.js 14) | TS | Labels only | Rebrand |
| Deploy/CI | `deploy/`, `.github/workflows/` | YAML | Image names | Rebrand |
| Skills | `skills/*.md` (13) | MD | Total | Rewrite for D365 |

**Total: ~21k LOC Rust + a Next.js UI.** The two SAP backend crates plus the
server tool layer (~10k LOC) are where the genuine engineering lives; the rest
is mechanical rename + content re-grounding.

---

## 2. Conceptual mapping — SAP S/4HANA → Dynamics 365

This table is the contract every later phase is checked against.

### Platform & connectivity

| SAP S/4HANA | Dynamics 365 (Finance & Operations / Dataverse) |
|---|---|
| S/4HANA application server | Dynamics 365 F&O environment (LCS / cloud-hosted) |
| RFC / BAPI (NetWeaver, SOAP RFC) | **OData v4** data entities, **Custom Services** (SOAP/JSON), `$batch` |
| `BAPI_TRANSACTION_COMMIT` (explicit commit) | OData `$batch` **change set** (atomic unit of work) |
| `BAPIRET2` return table | OData error payload + **Infolog** message list |
| DDIC tables (`ACDOCA`, `BSEG`, `BKPF`, `MARA`) | **Data entities** / backing tables (`GeneralJournalAccountEntry`, `LedgerJournalTrans`, `LedgerJournalHeader`, `ReleasedProductsV2`) |
| `MANDT` / `RCLNT` (client, first key) | `DataAreaId` (legal entity / company), `dataAreaId` OData property |
| Material number `MATN9` (CHAR 40, conversion exit) | **Item number** / Released Product `ItemNumber` (string) |
| Transport request (CTS, `E070`/`E071`) | **Deployable package** (LCS) / Dataverse **solution** import-export |
| ATC (ABAP Test Cockpit) | **Best Practices** checks + X++ compiler diagnostics; Power Platform **Solution Checker** |
| RAP (RESTful ABAP) service | F&O **OData data entity** / **Custom Service** group |
| CDS view | F&O **view** / **data entity** (also Dataverse view) |
| `USR02` / `AGR_*` / `RFCDES` | `SecurityRole` / `SecurityUserRole` / `SecurityDuty` / `SecurityPrivilege`; connection refs |
| Fiori app / launchpad | **Workspace** / model-driven app |
| SAP Help Portal | **Microsoft Learn** / `learn.microsoft.com/dynamics365` |
| SAP Business Accelerator Hub (sandbox OData) | D365 **public OData** (`/data`) + Microsoft sample/contoso data |

### Development metadata (the `adt` → `meta` rewrite)

| SAP ABAP / ADT | Dynamics 365 X++ / Metadata |
|---|---|
| ABAP class / interface / program / include | X++ **class** / **interface** / **job** / form / table |
| Function module | X++ **static method** / OData action |
| Package (DEVC) | **Model** / package within an AOT layer |
| ADT REST (`/sap/bc/adt/...`) | **Metadata OData** (`/metadata`), `EntityMetadatas`, `DataEntities`; X++ source over Git/VS Code |
| `where_used` (ADT) | **Cross-reference** (`xRef`) / Application Explorer "used by" |
| `activate` (ADT, write) | **Build & sync** / deploy package (gated write) |
| CDS view source | F&O view metadata via Metadata API |

### Domain workflows (the gated writes)

| SAP workflow tool | Dynamics 365 equivalent |
|---|---|
| `sap.workflow.create_purchase_order` (BAPI_PO_CREATE1) | `d365.workflow.create_purchase_order` → `PurchaseOrderHeadersV2` + `PurchaseOrderLinesV2` entities, `$batch` change set |
| `sap.workflow.maintain_customer_master` (BAPI customer) | `d365.workflow.maintain_customer` → `CustomersV3` entity (two-step elicitation) |
| `sap.workflow.release_transport` (CTS) | `d365.workflow.deploy_package` → LCS deployable package / solution import (re-typed confirmation) |

### Identity & auth

| SAP | Dynamics 365 |
|---|---|
| SAP logon ticket / basic auth | **Microsoft Entra ID** (Azure AD) OAuth 2.0 |
| XSUAA service key (BTP) | **App registration** client-credentials (`client_id`/`secret`/`tenant`) → token for `https://<env>.operations.dynamics.com` |
| `APIKey` header (Business Hub sandbox) | Bearer token (Entra) |

---

## 3. Naming conventions

| Source | Target |
|---|---|
| crate `sap-automate-rfc` | `d365-automate-odata` (F&O OData + Custom Service client) |
| crate `sap-automate-adt` | `d365-automate-meta` (X++/AOT metadata client) |
| crate `sap-automate-*` (others) | `d365-automate-*` |
| binary `sap-automate-server` | `d365-automate-server` |
| MCP tool `sap.*` | `d365.*` |
| MCP tool `abap.adt.*` | `xpp.meta.*` |
| MCP tool `sap.rfc.*` | `d365.odata.*` / `d365.service.*` |
| MCP tool `sap.table.*` | `d365.entity.*` |
| MCP tool `sap.bp.*` (Business Partner) | `d365.customer.*` |
| URI scheme `sap-system://`, `sap-table://`, `sap-rfc://`, `adt-destination://` | `d365-env://`, `d365-entity://`, `d365-service://`, `d365-meta://` |
| env `SAP_AUTOMATE_*`, `SAP_ODATA_*` | `D365_AUTOMATE_*`, `D365_ODATA_*` |
| company **ParagonCorp** | **GaussianTech** |
| ERP **SAP S/4HANA** | **Microsoft Dynamics 365** |

`mcp-*` crates keep their names (they are pure protocol).

---

## 4. Correctness invariants — re-grounded for D365

The source ships precision tests that fail loudly when SAP canon drifts. We
port the *spirit* of each to D365 canon:

| SAP precision test | D365 precision test |
|---|---|
| `every_write_bapi_has_bapiret2_in_tables` | `every_write_action_returns_operation_status` (Infolog / OData error contract) |
| `every_write_bapi_requires_commit` | `every_write_uses_batch_changeset` (no implicit commit; atomic `$batch`) |
| `every_rfc_has_at_least_one_authorization_entry` | `every_service_references_a_security_privilege` (duty/privilege) |
| `every_table_has_client_as_first_key` (`MANDT`) | `every_entity_is_company_scoped` (`DataAreaId`) |
| `material_number_is_char_40_per_s4hana` | `item_number_follows_released_product_convention` |
| `acdoca_is_present_and_marked_as_universal_journal` | `general_journal_account_entry_is_the_subledger_truth` |
| `compatibility_views_carry_s4hana_storage_note` | `legacy_table_maps_to_data_entity` (e.g. `BSEG`→`GeneralJournalAccountEntry` analog noted) |

---

## 5. Phased plan

Each phase ends **green**: `cargo build --all-features` + `cargo test` pass, and
the workspace is committed. Crates are added to the workspace `members` list
only as they land, so the tree always compiles.

### ✅ Phase 0 — Foundation & branding *(this commit)*
- Workspace skeleton: `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, `LICENSE` (Apache-2.0).
- This `PORTING.md`, a D365/GaussianTech `README.md`, `AGENTS.md`, `CHANGELOG.md`.
- **Exit:** `cargo build` succeeds on an empty-but-valid workspace.

### ✅ Phase 1 — MCP protocol core *(this commit)*
- Port `mcp-core`, `mcp-transport`, `mcp-server`, `mcp-client` verbatim; strip the handful of `sap` doc/env references.
- **Exit:** 18 MCP tests pass. ✅ *Done — see CHANGELOG 0.1.0.*

### ✅ Phase 2 — ERP-agnostic engines + agentic layer *(done)*
- Ported `rag`, `graph`, `kb`, `ingest`, `memory`, `scheduler`, `channels`, `observability`, `skills`, `connectors` as `d365-automate-*`.
- Re-grounded the domain surfaces in Dynamics 365 canon:
  - `graph::EntityKind` (`AbapObject`→`XppObject`, `Table`→`DataEntity`, `Rfc`→`Service`, `BpmnProcess`→`Flow`, `LeanixApp`→`Solution`, `HelpPage`→`LearnPage`) + `EdgeKind` (`ReadsTable`→`ReadsEntity`, `WritesTable`→`WritesEntity`, `Includes`→`Uses`).
  - The seed knowledge graph: the SAP FI journal-posting fixture (ABAP→BAPI→DDIC→LeanIX) re-cast as a Dynamics 365 GL posting fixture (`GTFinPostJournal` X++ job → `LedgerGeneralJournalEntry` service → `LedgerJournalTrans`/`GeneralJournalAccountEntry` entities → `FIN-CORE` solution).
  - `kb::Domain` (`SapHelp`→`Learn`, `Abap`→`Xpp`, `Bpmn`→`Flow`, `Leanix`→`Solution`).
  - Ingest crawler retargeted to Microsoft Learn (`HelpPortalCrawler`→`LearnCrawler`, `parse_help_portal_html`→`parse_learn_html`, `help.sap.com`→`learn.microsoft.com`); demo corpus re-grounded in D365 Finance.
  - Connector traits (`AbapConnector`→`XppConnector`, `BpmnConnector`→`FlowConnector`, `LeanixConnector`→`SolutionConnector`).
  - Observability audit field `sap_system`→`environment`; metrics `sap_rfc_calls_total`→`d365_service_calls_total`, etc.
- **Exit:** 86 tests pass, clippy clean. ✅ *Done — see CHANGELOG 0.2.0.*

### ✅ Phase 3 — Dynamics 365 backend tier *(done; live transports → 3b)*
- **`d365-automate-odata`** (was `sap-automate-rfc`): the `D365Client` trait (mirrors `SapClient`) + `MockD365Client` with GL/SCM/Sales fixtures; `$batch` change-set transaction model (replaces `BAPI_TRANSACTION_COMMIT`); `infolog` parser (replaces `bapiret2`); Entra ID OAuth2 credential providers; metadata-cache decorator; pool; retry/circuit-breaker; `D365Error` taxonomy.
- **`d365-automate-meta`** (was `sap-automate-adt`): the `MetadataClient` trait (mirrors `AdtClient`) + `MockMetadataClient` with the GTFin/GTScm X++ fixtures; `XppObjectKind` (AOT object kinds); cross-reference (`where_used`); gated `deploy` (`activate`); `MetaError` taxonomy; connection model with Entra auth.
- Re-grounded fixtures (entities, services, security) in D365 canon and ported the correctness invariants per §4 (`every_write_operation_uses_changeset`, `every_company_scoped_entity_has_dataareaid_key`, `general_journal_account_entry_is_the_subledger_truth`, …).
- **Exit:** 130 tests pass (44 new), clippy clean. ✅ *Done — see CHANGELOG 0.3.0.*

### ✅ Phase 3b — live transports *(done)*
- **`HttpD365Client`** (`d365-automate-odata`, feature `http`): live F&O OData v4 client over **Microsoft Entra ID** OAuth2 client-credentials (token cache), entity reads (`$select`/`$filter`/`$top` with `dataAreaId` injection, `like`→`contains` translation), unbound OData action POSTs, structured status→error mapping. Metadata/search/structure are served from the curated catalogue so the read-only safety annotations stay stable; data ops are live.
- **`HttpMetadataClient`** (`d365-automate-meta`, feature `http`): live Metadata API reads (`/metadata/...`, `/data/...`) over Entra OAuth2, plus a TOML **connection-file loader** (`load_connection`). `deploy` is gated and points to LCS; `cross_reference` is documented as not exposed over this transport.
- **Server wiring:** the binary selects live-vs-mock at runtime — `HttpD365Client::from_env()` when `D365_RESOURCE` is set, `HttpMetadataClient` when `--connection <name>` resolves to a non-mock connection file — otherwise the offline mocks. Backends remain `Arc<dyn …>` so the swap is one site.
- **Exit:** `http`-feature builds + 56 unit tests pass (URL/query/`$batch`/token/error builders, connection round-trip); clippy clean; live backend verified to activate via env (Entra OAuth2, secret redacted). CI exercises the `http` feature. ✅ *Done — see CHANGELOG 0.9.0.*

### ✅ Phase 4 — MCP server (tools / resources / prompts / seed) *(done)*
- `apps/d365-automate-server`: re-mapped the tool surface to `d365.*` / `xpp.meta.*` per §6 (37 tools across rag / service / meta / graph / workflow groups), resources to `d365-*://` (env, entity, service, meta, cache, guardrails), prompts (`d365.review-service-call`, `d365.deploy-impact-analysis`, `xpp.review-cross-reference` + disk-loaded skills), and the D365 seed corpus.
- Read-only-by-default exposure policy, `--enable-writes`, `AGENTS.md` loader, atomic `$batch` commit path on `d365.service.call`, audit logging, and the three elicitation workflows (`create_purchase_order`, `maintain_customer`, `deploy_package`).
- **Swappable backends (per request):** the server holds `Arc<dyn D365Client>` + `Arc<dyn MetadataClient>`, defaulting to the mocks. Pointing at a live environment is a one-site change in `lib.rs` / `main.rs` (construct the Phase 3b `HttpD365Client` instead of the mock) — no tool/resource/prompt code changes.
- **Exit:** binary runs (stdio + HTTP `/health`, `/metrics`, `/mcp`); 137 tests pass (7 new server integration tests), clippy clean. ✅ *Done — see CHANGELOG 0.4.0.*

### ✅ Phase 5 — Apps *(done)*
- Ported `d365-automate-tui` (Ratatui console), `d365-automate-gw` (multi-channel gateway), `d365-automate-ingest` (CLI), `d365-automate-bench`, and `sample-server`/`sample-client`.
- Re-grounded: TUI synthetic traffic + cache panel, gateway intent router / `match_skill` (D365 skills + keywords) and tool routing, bench workload queries (RAG + graph) and the `./docs/sample-learn-corpus`, ingest crawler (`LearnCrawler`). `sample-client`/`gw` spawn `d365-automate-server` by default.
- **Exit:** all six binaries build; clippy clean across `--all-targets`; bench passes both acceptance gates against the seeded Learn corpus (RAG P95 0.074 ms, graph multi-hop P95 0.084 ms); `sample-client` drives the server and lists the full D365 tool surface; 137 tests pass. ✅ *Done — see CHANGELOG 0.5.0.*

### ✅ Phase 6 — Web UI *(done)*
- Ported the Next.js 14 app (`apps/web`): Operations, Query Lab, Graph Lab, Tool Explorer, Skill Lab, Resources. Rebranded to D365-Automate; the `/api/mcp` proxy targets the D365 server.
- Re-grounded all demo content: tool names (`d365.*` / `xpp.meta.*`), the Operations tool grouping + synthetic traffic, the Query Lab domain selector (`learn`/`xpp`/`flow`/`solution`) + example queries + URI colour map, the Graph Lab entity-kind colours + examples, and the Skill Lab copy.
- **Exit:** `npm install` + `npx next build` succeed — all 9 routes compile and TypeScript type-checks pass. ✅ *Done — see CHANGELOG 0.6.0.*

### ✅ Phase 7 — Deploy & CI *(done)*
- `deploy/Dockerfile` (multi-stage, distroless/nonroot; builds `d365-automate-server` + `d365-automate-gw`), `deploy/k8s/*` (namespace, configmap with prod AGENTS.md, Entra-creds secret template, 3-replica deployment, ClientIP service, latency HPA 3–12, default-deny NetworkPolicy egress to HTTPS/Entra, PDB, Kustomize), `deploy/grafana/d365-automate-overview.json` (metrics rebranded), `deploy/d365-automate-connection.example.toml`.
- `.github/workflows/ci.yml` — fmt, clippy (`--all-targets`), test matrix (stable/beta), **Dynamics 365 correctness invariants** job (runs the 7 odata precision tests), bench acceptance gate (`d365-automate-bench --graph`), cargo-audit, Docker build, kubeconform, and the Next.js web build. `release.yml` — multi-arch binaries + GHCR image + GitHub Release. The Phase-3b `http` feature flags were dropped from CI (default build covers the mock path).
- **Exit:** k8s YAML + workflows + Grafana JSON all parse; the exact Docker build command (`--bin d365-automate-server --bin d365-automate-gw`) compiles; image refs consistent. ✅ *Done — see CHANGELOG 0.7.0.*

### ✅ Phase 8 — Skills & docs *(done)*
- Rewrote the 13 skills for Dynamics 365 (`skills/*.md`): `period_close_investigation` (ledger close), `xpp_code_review` (was abap-code-review), `extension_audit` (was clean-core-audit — over-layering vs extensions), `package_deploy_elicit` (was transport-release-elicit), `data_entity_design` (was odata-service-design), `deploy_impact_analysis` (was transport-impact-analysis), `synapse_link_migration` (was bw-to-datasphere), `custom_service_scaffolding` (was rap-service-scaffolding), `security_sod_audit`, `po_creation_elicit`, `customer_master_elicit`, `aipnv_ai_pairing`, `karpathy_guidelines` — all named `d365.skill.*` and wired to the D365 tool surface.
- Wired `main.rs` to scan `./skills` (+ `./.d365-automate/skills`, `~/.config/d365-automate/skills`) so each becomes an MCP prompt.
- Ported the docs: `docs/D365_CORRECTNESS.md`, `docs/INTEGRATION.md`, `docs/ROADMAP.md`, `docs/RUNBOOK_DEV_ENVIRONMENT.md`.
- **Exit:** the server loads **13 skills** → `prompts/list` shows **16 prompts** (13 skills + 3 built-ins), verified via `sample-client --list`. ✅ *Done — see CHANGELOG 0.8.0.*

---

## 6. Tool crosswalk (Phase 4 reference)

| SAP-Automate tool | D365-Automate tool | D365 backing |
|---|---|---|
| `sap.system.info` / `.health` / `.cache_stats` / `.cache_invalidate` | `d365.env.info` / `.health` / `.cache_stats` / `.cache_invalidate` | environment ping, metadata cache |
| `sap.rfc.search` / `.metadata` / `.bulk_metadata` / `.call` | `d365.service.search` / `.metadata` / `.bulk_metadata` / `.call` | Custom Service / OData action metadata + invoke |
| `sap.table.read` / `.structure` | `d365.entity.read` / `.structure` | OData entityset query + `$metadata` |
| `sap.bapi.parse_return` | `d365.infolog.parse` | Infolog / OData error parse |
| `sap.bp.search` / `.get` | `d365.customer.search` / `.get` | `CustomersV3` OData |
| `sap.docs.search` / `sap.help.search` / `sap.kb.navigate` | `d365.docs.search` / `d365.learn.search` / `d365.kb.navigate` | Microsoft Learn corpus |
| `abap.search` / `bpmn.find_process` / `eam.search_apps` | `xpp.search` / `flow.find_process` / `app.search_workspaces` | code/flow/workspace corpus |
| `abap.adt.get_{program,class,interface,include,function_module,cds_view,package_contents}` | `xpp.meta.get_{job,class,interface,form,method,view,model_contents}` | Metadata API |
| `abap.adt.search` / `.where_used` / `.get_table_contents` / `.activate` | `xpp.meta.search` / `.cross_reference` / `.get_entity_contents` / `.deploy` (gated) | Metadata API / xRef / build |
| `kb.multi_hop` / `kb.global_query` / `kb.summarise` / `kb.graph_neighborhood` | *(unchanged — engine is ERP-agnostic)* | graph engine |
| `sap.workflow.create_purchase_order` | `d365.workflow.create_purchase_order` | `PurchaseOrderHeadersV2`/`LinesV2` |
| `sap.workflow.maintain_customer_master` | `d365.workflow.maintain_customer` | `CustomersV3` |
| `sap.workflow.release_transport` | `d365.workflow.deploy_package` | LCS package / solution |

---

## 7. Risks & decisions

- **No NetWeaver-RFC analog.** D365 has no RFC; the `odata` crate is a genuine
  rewrite, not a rename. The `SapClient` trait becomes a `D365Client` trait with
  the same *shape* (search/metadata/read/call) so the server tool layer ports
  with minimal churn.
- **`$batch` vs explicit commit.** SAP's "call then `BAPI_TRANSACTION_COMMIT`"
  becomes "stage operations in a `$batch` change set, then submit atomically."
  The two-phase safety story (never auto-commit) is preserved.
- **Live-tenant testing.** Mirror the source's three tiers: CI (in-process axum
  mocks), demo (Microsoft public/sample OData where available), power-user
  (a real D365 dev environment via Entra app registration). Integration tests
  skip cleanly when `D365_*` secrets are unset.
- **Fidelity over speed.** Each phase lands compiling + tested; the workspace
  `members` list is the source of truth for what is done.
