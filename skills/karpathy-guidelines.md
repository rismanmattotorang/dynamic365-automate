---
name: d365.skill.karpathy_guidelines
description: Behavioural guidelines that reduce common LLM agent mistakes — surface assumptions, simplicity first, surgical changes, goal-driven execution. Apply to any D365-Automate task before you touch X++, data entities, or packages.
tags: [behaviour, guidelines, meta]
requires_tools: []
arguments:
  - name: task
    description: One-line description of the task at hand (e.g. "rewrite GTFinJournalPoster to use the LedgerGeneralJournalEntry service")
    required: true
---

# Pre-flight guidelines — applied to D365-Automate

Four principles that bias toward caution over speed. For trivial tasks
(one-shot entity lookup, a single `d365.docs.search`) skip to section 4 only.

The task you are about to perform: **{{task}}**.

## 1. Think before acting

Don't assume. Don't hide confusion. Surface tradeoffs.

Before invoking any write-side tool (`d365.service.call` with `commit=true`,
`xpp.meta.deploy`, any `d365.workflow.*`):

- **State your Dynamics 365 assumptions explicitly.** Which legal entity
  (DataAreaId)? Which fiscal calendar? Which deployable package? If uncertain,
  call the read-only tool first (`d365.env.info`, `d365.entity.read` on
  `CompaniesV2` / `FiscalCalendarPeriod`).
- **If multiple operations could satisfy the goal, present them.** A direct
  `d365.entity.read` often beats a custom service call. Don't pick silently.
- **If a simpler approach exists, say so.** `d365.docs.search` often beats a
  metadata dive.
- **If a precondition is unclear, stop.** Name what's confusing, and use the
  `d365.review-service-call` prompt to summarise the intended call first.

## 2. Simplicity first

Minimum tool calls that solve the problem. Nothing speculative.

- **No retrieval-layer escalation beyond what's needed.** Start with
  `d365.docs.search` (L2 hybrid). Promote to `kb.multi_hop` (L4 HippoRAG) *only*
  when the user explicitly asks about dependencies / impact / callers.
- **No unbounded entity reads.** Always set `fields` ($select). Always set a
  `filters` ($filter) clause for entities larger than ~1k rows. Scope by
  `dataAreaId`. Never raise `max_rows` above the default 100 unless asked.
- **No fabricated parameter defaults.** If the user hasn't supplied a legal
  entity / customer account / package ID, use the workflow tool's elicitation.

Ask: "Would a senior Dynamics 365 admin say this is overcomplicated?" If yes,
simplify.

## 3. Surgical changes

Touch only what the user asked you to touch. Clean up only your own mess.

When editing X++ via `xpp.meta.deploy`:
- **Don't "improve" adjacent code, comments, or formatting.**
- **Don't refactor things that aren't broken.**
- **Prefer extensions over over-layering** — Chain of Command, event handlers,
  and table/form extensions, never an over-layer of a standard object.
- **If you notice unrelated dead code, mention it — don't delete it.**

Always call `xpp.meta.cross_reference` first. Every changed line should trace
directly to the user's request or to an orphan your change created.

## 4. Goal-driven execution

Define success criteria up front. Loop until verified.

- "Investigate period close" → "List the open `FiscalCalendarPeriod` rows for
  the affected period, then map each to the canonical close procedure via
  `d365.docs.search`."
- "Add validation to GTFinJournalPoster" → "Write the validation, deploy to DEV,
  post a test journal that should fail, confirm it does."

For multi-step tasks, state a brief plan first:

```
1. <action> → verify: <check>
2. <action> → verify: <check>
```

## Acceptance checklist (paste into your final report)

- [ ] I stated my Dynamics 365 assumptions before any write-side call.
- [ ] I used the lowest retrieval layer that worked.
- [ ] I cited every claim with a `d365-learn://` / `d365-service://` /
      `d365-entity://` URI.
- [ ] My change touches only what was asked, and uses an extension (not over-layering).
- [ ] I ran `xpp.meta.cross_reference` before deploying any X++ object.
- [ ] No write-side tool was called without `--enable-writes` AND explicit user
      authorisation in the current session.
