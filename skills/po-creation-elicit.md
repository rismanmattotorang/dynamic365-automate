---
name: d365.skill.po_creation_elicit
description: Guided purchase-order creation with mid-execution elicitation for legal entity, vendor, item, and delivery date.
tags: [procurement, purchase-order, elicitation, workflow]
requires_tools: [d365.workflow.create_purchase_order, d365.service.metadata, d365.docs.search]
arguments:
  - name: vendor_hint
    description: Vendor account hint, e.g. "V-001"
    required: false
  - name: item_hint
    description: Released product item number hint, e.g. "D0001"
    required: false
---

Create a purchase order for **{{vendor_hint}}** / **{{item_hint}}** using the guided workflow.

The `d365.workflow.create_purchase_order` tool pauses mid-execution and asks the
user to confirm:

- Vendor account and released product (item number)
- Quantity
- **Legal entity (DataAreaId)** — high-stakes; never inferred silently
- Currency
- Requested delivery date

Steps:

1. Optionally run `d365.docs.search` with `"purchase order create PurchaseOrderHeadersV2"` to confirm the procedure.
2. Optionally run `d365.service.metadata` for `PurchaseOrderCreate` to confirm the parameter shape and that it `uses_changeset`.
3. Call `d365.workflow.create_purchase_order` with any vendor/item hints you have. The tool pauses and asks the user to confirm the form; the user can accept, decline (cancels with no side-effects), or cancel.
4. If accepted and the server was started with `--enable-writes`, the next step is `d365.service.call PurchaseOrderCreate` with `commit=true` (atomic `$batch` change set). Do NOT proceed without the user's explicit confirmation in the elicitation form.

Cite the service URI (`d365-service://PurchaseOrderCreate`) and the procedure page in the final summary.
