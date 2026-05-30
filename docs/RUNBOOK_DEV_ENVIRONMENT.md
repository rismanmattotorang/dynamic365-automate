# Runbook — operating against a Dynamics 365 dev environment

A short operator runbook for running the D365-Automate server, both offline
(mock) and against a live development environment.

## Offline (default)

```bash
cargo build --release
./target/release/d365-automate-server --transport http --bind 127.0.0.1:3030
curl http://127.0.0.1:3030/health      # → ok
curl http://127.0.0.1:3030/metrics     # → Prometheus exposition
```

The server runs read-only against the mocks. Drive it with any MCP client, or
the bundled `sample-client`:

```bash
./target/release/sample-client --server ./target/release/d365-automate-server --list
```

## Enabling writes

Write tools (elicitation workflows, `xpp.meta.deploy`, `commit=true` on
`d365.service.call`) are hidden from `tools/list` until you opt in:

```bash
./target/release/d365-automate-server --enable-writes
# or D365_AUTOMATE_ENABLE_WRITES=1
```

Even with writes enabled, the high-stakes workflows require an interactive
elicitation confirmation; a client without the `elicitation` capability cannot
trigger them.

## Live environment (Phase 3b)

1. Create a Microsoft Entra ID app registration and register it in the F&O
   environment (see [`INTEGRATION.md`](INTEGRATION.md)).
2. Export the credentials:
   ```bash
   export D365_RESOURCE="https://gt-dev.operations.dynamics.com"
   export D365_TENANT_ID="..." D365_CLIENT_ID="..." D365_CLIENT_SECRET="..."
   export D365_LEGAL_ENTITY="USMF"
   ```
3. (Phase 3b) start with `--connection dev` to use the live Metadata client.
   Until 3b ships, the server resolves and redacts the identity but serves the
   mock data — confirm via the `d365-env://info` resource.

## Health & observability

- `GET /health` → `ok`
- `GET /metrics` → Prometheus (`mcp_tool_*`, `rag_retrieval_latency_seconds`,
  `d365_service_calls_total`, `d365_pool_in_use`, `d365_authz_denied_total`).
- State-mutating tool calls are written to the audit log (`d365_audit` tracing
  target) with arguments redacted.
- Grafana dashboard: `deploy/grafana/d365-automate-overview.json`.

## Kubernetes

See [`../deploy/k8s/README.md`](../deploy/k8s/README.md): 3-replica deployment
on distroless/nonroot, ClientIP-affinity service, latency HPA (3–12),
default-deny NetworkPolicy (egress restricted to HTTPS / Entra), PDB.

## Skills

Drop a markdown file with YAML frontmatter into `./skills/` and restart — it
auto-loads as an MCP prompt (`prompts/list`). Thirteen ship in the repo; see
`skills/*.md`.

## Acceptance gates

```bash
./target/release/d365-automate-bench --n 1000 --graph
# RAG P95 < 80 ms; graph multi-hop P95 < 400 ms (against ./docs/sample-learn-corpus)
```
