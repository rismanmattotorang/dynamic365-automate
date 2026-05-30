---
name: d365.skill.custom_service_scaffolding
description: Generate the canonical scaffolding for a Dynamics 365 OData data entity or custom service (X++) over a table or view.
tags: [x++, data-entity, custom-service, scaffolding]
requires_tools: [xpp.meta.get_data_entity, xpp.meta.get_model_contents, xpp.meta.get_table, d365.docs.search]
arguments:
  - name: source
    description: Source table or view name, e.g. "LedgerJournalTrans"
    required: true
  - name: kind
    description: Scaffold kind (data_entity | custom_service)
    required: false
---

Scaffold a **{{kind}}** over **{{source}}**.

Read-only investigation phase (always run, even if writes are enabled):

1. **Inspect the source** — `xpp.meta.get_table` (or `xpp.meta.get_data_entity` if it already exists) for `{{source}}`. Extract fields, the natural key, and relations.
2. **Locate the parent model** — call `xpp.meta.get_model_contents` to find the model the source belongs to and any sibling artefacts.
3. **Avoid duplicates** — check whether a data entity / custom service for `{{source}}` already exists in the model.
4. **Procedure reference** — call `d365.docs.search` with `"create data entity public collection name"` (or `"custom service SysOperation OData action"`) to retrieve the canonical procedure.

Production phase (only when `--enable-writes` is active and the user confirms):

5. Produce a **plan** with:
   - Target entity name (`<Source>Entity` convention) + public collection name.
   - Key fields (include `DataAreaId` for company-scoped sources).
   - Data Management enabled? (yes for integration; no for read-only OData).
   - Security duties for read vs maintain.
6. Ask the user to confirm before invoking any write tool (`xpp.meta.deploy`).

Do NOT scaffold a writable entity over a ledger/subledger table until you have
called `d365.docs.search` with `"data entity write supported posting"` and
surfaced the relevant guidance.
