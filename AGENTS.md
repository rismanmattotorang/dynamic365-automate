# Agent Guardrails — D365-Automate

These rules apply to any AI agent driving this MCP server. They are ported from
the SAP-Automate guardrails and re-grounded in Microsoft Dynamics 365 semantics.

## Behavioural guidelines (apply before any tool call)

D365-Automate adopts four pre-flight guidelines. Run them as a mental check:

1. **Think before acting** — state your Dynamics 365 assumptions explicitly; if
   a simpler approach exists, say so; if a precondition is unclear, stop.
2. **Simplicity first** — minimum tool calls that solve the problem; no
   retrieval-layer escalation beyond what's needed; no unbounded entity reads;
   no fabricated parameter defaults.
3. **Surgical changes** — touch only what the user asked you to touch; clean up
   only your own mess; match existing style; mention unrelated dead code, never
   delete it.
4. **Goal-driven execution** — define success criteria up front; loop until
   verified; one bullet per step with an explicit `verify:` check.

## Read-only by default

- Production / UAT environments: use the read-side tools only —
  `d365.docs.search`, `d365.env.info`, `d365.env.health`, `d365.env.cache_stats`,
  `d365.env.cache_invalidate`, `d365.service.search`, `d365.service.metadata`,
  `d365.service.bulk_metadata`, `d365.entity.read`, `d365.entity.structure`,
  `d365.infolog.parse`, `d365.customer.search`, `d365.customer.get`,
  `d365.kb.navigate`, and the `xpp.meta.get_*` / `xpp.meta.search` /
  `xpp.meta.cross_reference` / `xpp.meta.get_entity_contents` readers.
- Do NOT call write-side OData actions (anything where `read_only=false` in its
  metadata) or `xpp.meta.deploy` unless the server was started with
  `--enable-writes` AND the user has explicitly authorised the change in the
  current session.
- The server hides write tools from `tools/list` entirely when in read-only
  mode. If you can see a write tool, the operator has opted in.

## Cite every claim

Every answer that references Dynamics 365 behaviour must cite either:
- a `d365-learn://` URI from `d365.docs.search`, OR
- a `d365-service://` URI from `d365.service.metadata`, OR
- a `d365-entity://` URI from `d365.entity.structure`.

## Before any `d365.service.call`

1. Invoke `d365.service.metadata` first to confirm the parameter signature.
2. Use the `d365.review-service-call` prompt to summarise the intended call.
3. Only then call `d365.service.call`.

## Before any `xpp.meta.deploy` (or any future write-side metadata tool)

1. Always call `xpp.meta.cross_reference` first to enumerate impacted callers.
2. Use the `xpp.review-cross-reference` prompt to structure the impact summary.
3. Only then deploy.

## Writes are atomic — never auto-commit

Dynamics 365 writes are staged into an OData `$batch` **change set** and
submitted as a single atomic unit of work — there is no implicit commit. The
agent never partially-applies a write.

## Workflow tools use elicitation — never fabricate confirmations

High-stakes workflows pause mid-execution and ask the user to confirm legal
entity, customer account, or deployment package via a structured form rendered
by the client:

- `d365.workflow.create_purchase_order`
- `d365.workflow.maintain_customer` (chained two-step elicitation)
- `d365.workflow.deploy_package` (re-typed confirmation phrase)

The agent's role is to *kick off the workflow* with the best hints it has —
never to hard-code legal entities, customer accounts, or package IDs. If the
user declines or the client lacks the elicitation capability, the tool aborts
safely with no write side-effect.

## Choose the right retrieval layer

| Layer | Tool | When |
|---|---|---|
| **L2 Hybrid** | `d365.docs.search` | Default. Lexical + semantic + RRF + rerank over the document corpus. |
| **L3 GraphRAG** | `kb.global_query` | Global / analytical questions. Returns community summaries spanning domains. |
| **L4 HippoRAG** | `kb.multi_hop` | Multi-hop / impact / where-used queries. PPR-ranked, hop-distance-bounded. |
| **L5 RAPTOR** | `kb.summarise` | Granularity-aware orientation across module roll-ups. |

When in doubt, start with `d365.docs.search`. Promote to `kb.multi_hop` only
when the user explicitly asks about dependencies, impact, or callers.

## Entity reads

- Always set `fields` (`$select` projection) — do not fetch all columns by default.
- Always set a `filter` (`$filter`) clause for entities larger than ~1k rows.
- Always scope by `DataAreaId` (legal entity) when the entity is company-specific.
- Never raise `max_rows` (`$top`) above the default 100 unless the user requests it.
