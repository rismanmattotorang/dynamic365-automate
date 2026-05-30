---
name: d365.skill.aipnv_ai_pairing
description: AI-Pairing-Not-Vibing (AIPNV) pre-flight checklist — anti-autopilot guardrails for Dynamics 365 write operations. Forces an explicit human-in-the-loop confirmation before package deployments, OData write actions, or X++ deploys.
tags: [behaviour, guardrails, aipnv, safety]
requires_tools: [d365.env.info, xpp.meta.cross_reference, d365.review-service-call]
arguments:
  - name: intended_action
    description: One-line description of the write you are about to perform (e.g. "deploy GTFinJournalPoster", "deploy package GTFin-2026.04", "post a general journal")
    required: true
---

# AI-Pairing-Not-Vibing (AIPNV)

A pre-flight checklist that keeps the agent out of autopilot before any
state-mutating Dynamics 365 operation.

D365-Automate enforces AIPNV at three layers in the runtime: exposure policy
(write tools hidden in read-only mode), the per-call `read_only=false` flag,
and AGENTS.md guardrails surfaced in `initialize.instructions`. This skill is
the **fourth** layer — the agent's own pre-flight checklist, run before the write.

**Intended action:** {{intended_action}}

## The five-question checklist

Answer every question explicitly in your reply to the user. If you cannot
answer one of them, **stop** and ask the user before invoking the write tool.

### Q1. What environment am I targeting?

Call `d365.env.info`. State the `environment` / `legal_entity` /
`environment_role` (`DEV` / `UAT` / `PROD`) verbatim in your reply.

**Stop conditions:**
- `environment_role == "PROD"` and the user has not explicitly authorised a
  production write in *this* session (not a prior session, not inferred).
- `environment` does not match the one the user named in their request.

### Q2. What is the blast radius?

For X++ deploys, call `xpp.meta.cross_reference` on the target object and quote
the impacted-caller count. For OData write actions, name every data entity the
operation touches (e.g. posting a general journal ⇒ `LedgerJournalTrans` +
`GeneralJournalAccountEntry`). For package deployments, call
`xpp.meta.get_model_contents` to enumerate the changed objects.

**Stop conditions:**
- More than 50 callers and the user has not acknowledged the scope.
- The change over-layers SAP-standard… er, Microsoft-standard objects instead
  of using an extension (see `d365.skill.extension_audit`).

### Q3. What is the rollback path?

Name it explicitly:
- X++ deploy: redeploy the prior package / revert the Git commit and rebuild.
- OData posting: the reversing action (e.g. a reversing general journal) and
  the document key needed.
- Package deployment: re-apply the previous deployable package via LCS.

**Stop condition:** no rollback path. Do not proceed.

### Q4. Have I cited the Dynamics 365 canon for this operation?

Call `d365.docs.search` for the operation / entity / procedure. Cite the
returned `d365-learn://` URI in your reply. If the docs contradict your
intended call signature, fix the call — don't override the canon.

### Q5. Has the user explicitly authorised this write in this session?

Re-read the most recent user turn. The authorisation must be **explicit**
(matches the action), **scoped** (not a blanket "do whatever"), and **current**
(this session). If any is false, **invoke the elicitation flow** via the
matching workflow tool (`d365.workflow.create_purchase_order`,
`d365.workflow.maintain_customer`, `d365.workflow.deploy_package`) which renders
a structured confirmation form on the client. Never fabricate the confirmation.

## Final gate

Only after Q1–Q5 are answered may you invoke the write tool. Include the answers
in your final report so the audit log captures them alongside the call.
