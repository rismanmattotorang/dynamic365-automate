# D365-Automate — production readiness assessment

> **Goal:** get the codebase to a state where it can be tested against a real
> Dynamics 365 Finance & Operations development environment — not the offline
> mocks, a live tenant.
>
> **Status (2026-05-30):** all eight porting phases plus the live HTTP
> transports (3b) are complete. 149 tests pass; clippy clean across
> `--all-targets` (incl. the `http` feature); the Next.js build is green; the
> server runs over stdio + HTTP.

---

## 1. Claim vs. reality

The architecture, MCP surface, RAG/graph engines, packaging, and the live HTTP
clients are in place. The remaining gap is *exercising* the live path against a
real environment with credentials.

| Capability | Framing | Actual state | Real against a dev environment? |
|---|---|---|---|
| Entity reads (`d365.entity.read`) | live OData v4 | `HttpD365Client` is real reqwest code over Entra OAuth2; selected at runtime when `D365_RESOURCE` is set | ⚠️ Code complete; needs a tenant + credentials to verify |
| Service calls (`d365.service.call`) | OData action POST | Real; single-action POST is atomic server-side | ⚠️ As above |
| Atomic writes | `$batch` change set | Single-op POST today; multi-op change-set batching is the follow-up | ⚠️ Partial |
| Metadata reads (`xpp.meta.*`) | live Metadata API | `HttpMetadataClient` real; selected via `--connection <file>` | ⚠️ Needs a tenant |
| Deploy (`xpp.meta.deploy`) | gated write | Gated; live path returns "use LCS" (deployment is out-of-band) | ✅ Correct by design |
| Cross-reference (`xpp.meta.cross_reference`) | impact analysis | Mock returns wired fixtures; live returns empty (xRef DB not exposed over Metadata OData) | ⚠️ Documented limitation |
| Everything else (retrieval, graph, skills, server, apps, web, deploy, CI) | shipped | Real, tested | ✅ |

**Bottom line:** the live clients exist and are wired; what remains is running
them against a real environment (which needs an Entra app registration and a
non-public tenant) and hardening the multi-operation write path.

## 2. Sprint plan to "verified on a dev environment"

### Sprint 1 — Connectivity proof
- Register an Entra ID app; grant it to a dev F&O environment (see
  [`INTEGRATION.md`](INTEGRATION.md)).
- Export `D365_*` env / drop a connection file; confirm `d365.env.info`,
  `d365.entity.read` against `CompaniesV2` / `ReleasedProductsV2`.
- Add live integration tests gated on `D365_*` secrets so CI without secrets
  skips cleanly.

### Sprint 2 — Write path
- Implement multi-operation atomic `$batch` change sets (today a single OData
  action POST is atomic; multi-op is the follow-up).
- Verify `d365.service.call ... commit=true` posts a test general journal and
  reverses it; confirm the Infolog/`$batch` rollback semantics end-to-end.

### Sprint 3 — Hardening
- Entra certificate credential (currently client-secret + bearer).
- Real embedding provider (Azure OpenAI / text-embedding-3-large) behind the
  `EmbeddingClient` trait; ONNX cross-encoder reranker.
- Cross-reference via the build-time xRef database (out-of-band ingest).

## 3. Risk register

| ID | Risk | Mitigation |
|---|---|---|
| R1 | Throttling on the OData endpoint under load | retry/circuit-breaker already in `d365-automate-odata`; back off on 429/503 |
| R2 | Token expiry mid-session | token cache refreshes 60 s early |
| R3 | Over-broad Entra app privileges | scope the service account to the duties the tool surface needs; run `d365.skill.security_sod_audit` |
| R4 | Cross-company data leak | `DataAreaId` injection on every company-scoped read; `every_company_scoped_entity_has_dataareaid_key` invariant in CI |
