---
name: d365.skill.customer_master_elicit
description: Two-step elicitation for customer master maintenance — pick the view, then fill scoped fields.
tags: [sales, customer-master, elicitation, workflow]
requires_tools: [d365.workflow.maintain_customer, d365.docs.search]
arguments:
  - name: customer_hint
    description: Customer account hint, e.g. "US-001"
    required: false
---

Maintain customer master data for **{{customer_hint}}** using the chained elicitation workflow.

The `d365.workflow.maintain_customer` tool issues **two elicitations**:

1. **Scope selection** — which view to maintain (general | credit_collections | sales_demographics) and the legal entity.
2. **Scoped fields** — the form fields depend on the chosen view:
   - *general*: organization name, customer group, country/region
   - *credit_collections*: credit limit, terms of payment, hold status
   - *sales_demographics*: sales currency, default site, mode of delivery

Steps:

1. Search the docs first with `d365.docs.search` and `"customer master CustomersV3 maintain"` to confirm the canonical procedure.
2. Call `d365.workflow.maintain_customer`. Walk the user through the two elicitations.
3. Echo the confirmed changes back to the user before the (write-mode-gated) `d365.service.call CustomerMaintain` with `commit=true`.

This skill demonstrates **chained elicitation** — declining the first form aborts cleanly without ever showing the second.
