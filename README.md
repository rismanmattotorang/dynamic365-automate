<div align="center">

# D365-Automate

### The agentic OS for Microsoft Dynamics 365 — built in Rust, on-premise by default.

**Sub-millisecond retrieval. MCP-native. Apache-2.0.**
**Made by [GaussianTech](#about-gaussiantech).**

[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![MCP](https://img.shields.io/badge/MCP-2025--06--18-8b5cf6?style=flat-square)](https://modelcontextprotocol.io)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue?style=flat-square)](LICENSE)
[![Status](https://img.shields.io/badge/status-porting%20in%20phases-f59e0b?style=flat-square)](PORTING.md)

[**Porting strategy →**](PORTING.md) · [Why it exists](#why-gaussiantech-built-this) · [Architecture](#architecture) · [Status](#porting-status)

</div>

---

> **D365-Automate is a faithful port of [`sap-automate`](https://github.com/rismanmattotorang/sap-automate)**
> — ParagonCorp's agentic OS for SAP S/4HANA — re-targeted onto **Microsoft
> Dynamics 365** Finance & Operations and built for **GaussianTech**. The
> MCP protocol core and the RAG / graph retrieval engines port nearly verbatim;
> the SAP backend tier (RFC / BAPI / DDIC / ADT) is rewritten against the
> Dynamics 365 backend tier (OData v4 / Custom Services / Data Entities /
> Metadata API). The full plan lives in **[`PORTING.md`](PORTING.md)**.

---

## Porting status

| Phase | Scope | State |
|---|---|---|
| 0 | Foundation & branding | ✅ Done |
| 1 | MCP protocol core (`mcp-core/-transport/-server/-client`) | ✅ Done — 18 tests passing |
| 2 | ERP-agnostic engines + agentic layer (`kb`/`rag`/`graph`/`ingest`/`memory`/`scheduler`/`channels`/`skills`/`observability`/`connectors`) | ✅ Done — 86 tests passing |
| 3 | Dynamics 365 backend tier (`d365-automate-odata`, `d365-automate-meta`) | ⏳ Planned |
| 4 | MCP server: tools / resources / prompts / seed | ⏳ Planned |
| 5 | Apps (TUI, gateway, ingest, bench, samples) | ⏳ Planned |
| 6 | Web UI (Next.js) | ⏳ Planned |
| 7 | Deploy & CI | ⏳ Planned |
| 8 | Skills & docs | ⏳ Planned |

See **[`PORTING.md`](PORTING.md)** for the phase-by-phase strategy, the full
SAP→Dynamics 365 conceptual mapping, the tool crosswalk, and the correctness
invariants.

---

## Quick start (current foundation)

```bash
# Build the MCP protocol foundation (Rust 1.80+).
cargo build --all-features

# Run the protocol test suite.
cargo test --all-features
```

The Dynamics 365 server binary, tools, and web UI land in later phases (see the
status table). Until then the workspace ships the protocol core that everything
else is built on.

---

## Why GaussianTech built this

Microsoft Dynamics 365 Finance & Operations runs the financials, supply chains,
and operations of a large slice of the mid-market and enterprise. But the gap
between *what AI agents can do generally* and *what they can do against Dynamics
365* is real: existing connectors are cloud-locked, drift from the published
OData / Metadata canon, and ship in stacks with 10–100 ms latency tails.

**D365-Automate closes that gap — on-prem capable, in Rust, with the correctness
story written down in tests.** GaussianTech runs a large Dynamics 365 estate and
built this for its own operations first, then open-sourced it under Apache-2.0.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│  Channels: Teams · Slack · Telegram · Email · CLI                    │  d365-automate-channels
├──────────────────────────────────────────────────────────────────────┤
│  Gateway: intent routing · 4-tier memory · proactive scheduler       │  d365-automate-gw
├──────────────────────────────────────────────────────────────────────┤
│  MCP transports: stdio · HTTP+SSE · Streaming HTTP                   │  mcp-transport ✅
├──────────────────────────────────────────────────────────────────────┤
│  MCP server: tools · resources · prompts · elicitation               │  mcp-server ✅ + apps/d365-automate-server
├──────────────────────────────────────────────────────────────────────┤
│  RAG engine: dense + BM25 + RRF + cross-encoder reranker             │  d365-automate-rag
│  Graph engine: GraphRAG (Louvain) · HippoRAG (PPR) · RAPTOR          │  d365-automate-graph
├──────────────────────────────────────────────────────────────────────┤
│  Knowledge base: in-memory · Qdrant · ArangoDB · DocumentTree        │  d365-automate-kb
│  Ingestion: HTML crawler · contextual chunker · embedding pipeline   │  d365-automate-ingest
├──────────────────────────────────────────────────────────────────────┤
│  D365 backends: D365Client · MetadataClient (HTTP + mock)            │  d365-automate-odata · d365-automate-meta
│  Auth: Microsoft Entra ID (OAuth2 client-credentials)                │
├──────────────────────────────────────────────────────────────────────┤
│  Observability: Prometheus · audit log · OpenTelemetry ready         │  d365-automate-observability
└──────────────────────────────────────────────────────────────────────┘
```

✅ = ported and tested in the current commit. The rest lands per
[`PORTING.md`](PORTING.md).

---

## About GaussianTech

**GaussianTech** is a technology enterprise that runs a large Microsoft Dynamics
365 estate across finance and operations. Its platform engineering team builds
and operates the AI tooling that GaussianTech's own Dynamics organisation
depends on, and open-sourced D365-Automate under Apache-2.0 to reduce the cost
of fragmentation across the Dynamics 365 ecosystem.

---

## Credits

This project is a port of [`rismanmattotorang/sap-automate`](https://github.com/rismanmattotorang/sap-automate)
(ParagonCorp, Apache-2.0). The MCP protocol design, RAG/graph architecture, and
agentic gateway are carried over with attribution.

---

## License

[Apache-2.0](LICENSE). Use it, fork it, build a business on top of it.

---

<div align="center">

**GaussianTech** · Platform Engineering · 2026

</div>
