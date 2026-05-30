<div align="center">

# D365‑Automate

### The agentic runtime for Microsoft Dynamics 365.

**Give any AI agent safe, sub‑millisecond, citation‑grade control of your ERP — in Rust, on your own infrastructure.**

*by [**Gaussian Technologies**](#about-gaussian-technologies)*

[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![MCP](https://img.shields.io/badge/MCP-2025--06--18-8b5cf6?style=flat-square)](https://modelcontextprotocol.io)
[![Auth](https://img.shields.io/badge/auth-Microsoft%20Entra%20ID-0078d4?style=flat-square)](https://learn.microsoft.com/entra/)
[![Tests](https://img.shields.io/badge/tests-149%20passing-22c55e?style=flat-square)](#proof)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue?style=flat-square)](LICENSE)

[Why it exists](#the-gap) · [What it is](#what-it-is) · [Proof](#proof) · [Architecture](#architecture) · [Quick start](#quick-start) · [Deploy](#deploy)

</div>

---

## The gap

Dynamics 365 Finance & Operations runs the ledgers, supply chains, and order books of a large slice of the mid‑market and enterprise. The moment you point an AI agent at it, three things break:

- **It hallucinates the ERP.** No grounding in the live OData metadata, the data‑entity schema, or the security model — so it invents fields, operations, and postings.
- **It isn't safe to let write.** A general agent has no concept of read‑only‑by‑default, of legal‑entity scoping, of an atomic unit of work, or of a human‑in‑the‑loop gate before it posts a journal.
- **It's slow and cloud‑locked.** Connector stacks add 10–100 ms latency tails and assume your data leaves your tenant.

**D365‑Automate closes that gap.** It is the runtime that sits between the agent and Dynamics 365 and makes ERP operations *fast, grounded, and safe by construction.*

## What it is

An **agentic operating system for Dynamics 365**, exposed over the [Model Context Protocol](https://modelcontextprotocol.io) (2025‑06‑18). Drop it into Claude, Cursor, or any MCP client and the agent gets a typed, guard‑railed surface over your environment:

- **37 production tools** across OData services, data entities, X++/AOT metadata, a cross‑domain knowledge graph, and gated write workflows.
- **12 resources** and **16 prompts** — including **13 declarative skills** (period‑close investigation, SoD audit, extension audit, X++ review, deploy‑impact analysis, …) loaded straight from disk.
- **Sub‑millisecond hybrid retrieval** (dense + BM25 + RRF + cross‑encoder rerank) and **multi‑hop graph reasoning** (GraphRAG · HippoRAG · RAPTOR).

### Built on three convictions

| | |
|---|---|
| **Safe by construction** | Read‑only by default — write tools are *hidden* until you opt in. Every write is staged into an atomic **OData `$batch` change set** (never auto‑commits, fail‑closed on unconfirmed outcomes) and high‑stakes workflows pause for a typed, human‑in‑the‑loop **elicitation**. Every company‑scoped read is pinned to its **`DataAreaId`** so cross‑entity leaks are impossible. Every mutation is audited. |
| **Grounded in canon** | Service metadata, entity structure, and the security model come from a curated catalogue with correctness invariants enforced in CI; answers cite a `d365‑learn://`, `d365‑service://`, or `d365‑entity://` source URI. |
| **Yours to run** | A single Rust binary. On‑prem, in your VPC, or on Kubernetes — distroless, nonroot, default‑deny network policy. Authenticated with **Microsoft Entra ID**. Your data never has to leave your tenant. |

## Proof

> One repository. Verifiable claims.

- **Sub‑millisecond retrieval.** On the bundled corpus, hybrid RAG **P95 = 0.074 ms** (gate < 80 ms) and 4‑hop graph traversal **P95 = 0.084 ms** (gate < 400 ms) — `cargo run -p d365-automate-bench --release -- --graph`.
- **Correctness in tests.** **149 tests** pass; seven Dynamics 365 correctness invariants run as a dedicated CI gate (atomic change sets, `DataAreaId` scoping, the universal‑journal source of truth, item‑number convention, …). See [`docs/D365_CORRECTNESS.md`](docs/D365_CORRECTNESS.md).
- **Real transports.** stdio + HTTP/SSE; live OData v4 and Metadata API clients over Entra OAuth2 behind the `http` feature; the offline mock backs CI and local dev with zero credentials.

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│  Channels: Teams · Slack · Telegram · Email · CLI                    │  d365-automate-channels
├──────────────────────────────────────────────────────────────────────┤
│  Gateway: intent routing · 4-tier memory · proactive scheduler       │  d365-automate-gw
├──────────────────────────────────────────────────────────────────────┤
│  MCP transports: stdio · HTTP+SSE                                    │  mcp-transport
├──────────────────────────────────────────────────────────────────────┤
│  MCP server: 37 tools · 12 resources · 16 prompts · elicitation      │  d365-automate-server
├──────────────────────────────────────────────────────────────────────┤
│  RAG engine: dense + BM25 + RRF + cross-encoder reranker             │  d365-automate-rag
│  Graph engine: GraphRAG (Louvain) · HippoRAG (PPR) · RAPTOR          │  d365-automate-graph
├──────────────────────────────────────────────────────────────────────┤
│  Knowledge base: in-memory · Qdrant · DocumentTree                   │  d365-automate-kb
│  Ingestion: crawler · contextual chunker · embedding pipeline        │  d365-automate-ingest
├──────────────────────────────────────────────────────────────────────┤
│  D365 backends: OData v4 + Custom Service · Metadata/AOT API         │  d365-automate-odata · d365-automate-meta
│  Atomic $batch change sets · Microsoft Entra ID OAuth2               │
├──────────────────────────────────────────────────────────────────────┤
│  Observability: Prometheus · audit log · OpenTelemetry-ready         │  d365-automate-observability
└──────────────────────────────────────────────────────────────────────┘
```

A typed cross‑domain **knowledge graph** stitches X++ objects, OData services, data entities, Power Automate flows, Dataverse solutions, and Learn pages into one substrate — so an agent can answer *"what posts to `GeneralJournalAccountEntry`, and what breaks if I change it?"* with a multi‑hop, cited traversal.

## Quick start

```bash
# Build everything (Rust 1.80+).
cargo build --release

# Single binary, stdio MCP server — drop into Claude Code, Cursor, or any MCP client.
./target/release/d365-automate-server

# Or HTTP for browser / remote agents.
./target/release/d365-automate-server --transport http --bind 127.0.0.1:3030
curl http://127.0.0.1:3030/health      # → ok
curl http://127.0.0.1:3030/metrics     # → Prometheus exposition

# Opt in to the gated write tools (elicitation workflows, atomic $batch commits).
./target/release/d365-automate-server --enable-writes
```

It runs against **offline Dynamics 365 mocks** out of the box — fully exercisable with zero credentials.

### Connect a live environment

```bash
export D365_RESOURCE="https://gt-prod.operations.dynamics.com"
export D365_TENANT_ID=... D365_CLIENT_ID=... D365_CLIENT_SECRET=... D365_LEGAL_ENTITY=USMF
./target/release/d365-automate-server --transport http
```

The server acquires an Entra ID token and serves live entity reads and OData
actions. Full setup (app registration, connection files): [`docs/INTEGRATION.md`](docs/INTEGRATION.md).

### Operator console

```bash
cd apps/web && npm install && npx next dev   # → http://localhost:3000
```

Six routes: Operations, Query Lab, Graph Lab, Tool Explorer, Skill Lab, Resources.

## Deploy

```bash
docker build -t ghcr.io/gaussiantech/d365-automate:$(git rev-parse --short HEAD) -f deploy/Dockerfile .
kubectl apply -k deploy/k8s/
```

Production manifests in [`deploy/k8s/`](deploy/k8s/README.md): 3‑replica deployment on distroless + nonroot, ClientIP‑affinity service, latency HPA (3–12), default‑deny NetworkPolicy with egress restricted to HTTPS/Entra, PodDisruptionBudget, Kustomize. Grafana dashboard included.

## Engineering

A Rust workspace — protocol core, retrieval/graph engines, the Dynamics 365 backend tier, the MCP server, five companion binaries (TUI, gateway, ingest, bench, samples) — plus a Next.js console. CI runs fmt, clippy (`-D warnings`, all targets, incl. the `http` feature), a stable/beta test matrix, the correctness‑invariants gate, the latency acceptance gate, cargo‑audit, a Docker build, kubeconform, and the web build.

D365‑Automate is a faithful port of the open‑source [`sap-automate`](https://github.com/rismanmattotorang/sap-automate) architecture, re‑grounded from SAP S/4HANA onto Microsoft Dynamics 365. The phase‑by‑phase strategy and the full SAP→Dynamics 365 mapping live in [`PORTING.md`](PORTING.md); the changelog in [`CHANGELOG.md`](CHANGELOG.md); the roadmap in [`docs/ROADMAP.md`](docs/ROADMAP.md).

## About Gaussian Technologies

**Gaussian Technologies** builds the infrastructure that lets enterprises put AI agents to work on the systems that actually run the business. We run a large Dynamics 365 estate ourselves; we built D365‑Automate for our own finance and operations teams first, then open‑sourced it under Apache‑2.0 — because the cost of every enterprise re‑solving "safe agentic ERP" in isolation is too high to bear alone.

## License

[Apache‑2.0](LICENSE). Use it, fork it, build a business on top of it.

---

<div align="center">

**Gaussian Technologies** · Platform Engineering · 2026

</div>
