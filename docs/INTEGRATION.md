# Integration — wiring a live Dynamics 365 environment

D365-Automate ships against **offline mocks** by default (`MockD365Client`,
`MockMetadataClient`), so the full MCP surface is exercisable with no
environment. This document describes how a live Finance & Operations environment
is wired.

> **Status:** the live HTTP clients land in **Phase 3b** (behind the `http`
> feature). The seams below are in place today; until 3b ships, the server runs
> against the mocks. See [`../PORTING.md`](../PORTING.md) §3b.

---

## 1. Microsoft Entra ID app registration

Dynamics 365 F&O authenticates with **Microsoft Entra ID** (Azure AD) using the
OAuth 2.0 client-credentials grant.

1. In Entra ID, register an application; create a **client secret** (or
   certificate).
2. In the F&O environment, register the app as an **Azure Active Directory
   application** (System administration → Setup → Microsoft Entra ID
   applications) and map it to a service account with the required duties.
3. The app exchanges its credentials at
   `https://login.microsoftonline.com/<tenant_id>/oauth2/v2.0/token` for a
   bearer token scoped to the environment resource
   `https://<env>.operations.dynamics.com`.

This replaces SAP logon tickets / XSUAA service keys.

## 2. Credentials (env-driven)

The `EnvCredentialProvider` reads:

| Variable | Example |
|---|---|
| `D365_RESOURCE` | `https://gt-prod.operations.dynamics.com` |
| `D365_TENANT_ID` | Entra directory (tenant) id |
| `D365_CLIENT_ID` | app registration (client) id |
| `D365_CLIENT_SECRET` | app registration secret |
| `D365_LEGAL_ENTITY` | default `DataAreaId`, e.g. `USMF` |

Credentials are layered (`LayeredCredentialProvider`): env first, then a static
fallback for the offline demo. The secret is never logged — only the redacted
identity (`client_secret: "***"`) appears in `d365-env://info`.

## 3. Connection file (metadata tier)

The Metadata API tier (`d365-automate-meta`) selects a named connection. Copy
the template and select it with `--connection <name>`:

```bash
cp deploy/d365-automate-connection.example.toml \
   ./.d365-automate/connections/dev.toml
# edit base_url + [auth], then:
./target/release/d365-automate-server --connection dev
```

`./.d365-automate/` is gitignored — keep credentials out of version control and
prefer a secrets manager in production.

## 4. Endpoints

| Surface | Path | Crate |
|---|---|---|
| Data entities (OData v4) | `GET/POST <resource>/data/<EntitySet>` | `d365-automate-odata` |
| Atomic writes | `POST <resource>/data/$batch` (change set) | `d365-automate-odata` |
| Custom Service operations | `POST <resource>/api/services/...` | `d365-automate-odata` |
| Metadata (X++ / AOT) | `GET <resource>/metadata/...` | `d365-automate-meta` |

## 5. Swapping the mock for a live client

Every backend is held behind a trait object (`Arc<dyn D365Client>`,
`Arc<dyn MetadataClient>`). Pointing at a live environment is a **one-site
change** in `apps/d365-automate-server/src/lib.rs` / `main.rs`: construct the
Phase-3b `HttpD365Client` instead of `MockD365Client` and assign it to the same
`Arc<dyn …>`. No tool, resource, or prompt code changes.

## 6. Three-tier testing

Mirroring the source project's strategy:

1. **CI tier** — in-process mocks (ship today; no network).
2. **Demo tier** — Microsoft public/sample OData where available.
3. **Power-user tier** — a real F&O dev environment via the Entra app
   registration above.

Live integration tests skip cleanly when the `D365_*` secrets are unset, so CI
without secrets is unaffected.
