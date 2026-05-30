---
name: d365.skill.period_close_investigation
description: Investigate root causes of a Dynamics 365 ledger period-close delay or block.
tags: [finance, period-close, investigation]
requires_tools: [d365.docs.search, d365.entity.read, d365.service.metadata]
arguments:
  - name: legal_entity
    description: Legal entity (DataAreaId), e.g. "USMF"
    required: true
  - name: fiscal_period
    description: Fiscal period being closed, e.g. "2026-M03"
    required: false
---

Investigate why the ledger period close for **{{legal_entity}}** ({{fiscal_period}}) is delayed.

Work through the following steps and cite every claim with a `d365-learn://`,
`d365-service://`, or `d365-entity://` URI:

1. **Procedure baseline** — call `d365.docs.search` with `"financial period close foreign currency revaluation"` to retrieve the canonical procedure. Confirm the standard order: close sub-ledgers → run foreign-currency revaluation → reconcile → close the ledger period.
2. **Period state** — call `d365.entity.read` on `FiscalCalendarPeriod` filtered by `dataAreaId eq '{{legal_entity}}'` to confirm the periods are Open / OnHold / Closed as expected.
3. **Reconciliation status** — call `d365.docs.search` with `"LedgerJournalTrans GeneralJournalAccountEntry reconciliation"` to retrieve the sub-procedure; flag any discrepancies the user reported against this baseline.
4. **Posting blockers** — if a journal post is failing, call `d365.service.metadata` for `LedgerGeneralJournalEntryPost` and report the parameter shape and required security the validation expects.
5. **Summary** — produce a 3-section report: *What's blocking*, *Recommended remediation*, *Pre-close checklist for next month*.

Do NOT call any state-modifying operation. If the user authorises a remediation,
propose it but require explicit confirmation before invoking `d365.service.call`
with `commit=true`.
