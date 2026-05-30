# Changelog

All notable changes to **D365-Automate** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

D365-Automate is a port of [`sap-automate`](https://github.com/rismanmattotorang/sap-automate)
(ParagonCorp, Apache-2.0) re-targeted onto Microsoft Dynamics 365 for
GaussianTech. See [`PORTING.md`](PORTING.md) for the phased strategy.

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
