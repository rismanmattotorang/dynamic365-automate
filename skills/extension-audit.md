---
name: d365.skill.extension_audit
description: Audit a Dynamics 365 model against the extensibility principles — no over-layering, use extensions (Chain of Command, event handlers, table/form extensions).
tags: [extensibility, x++, audit, compliance]
requires_tools: [xpp.meta.get_model_contents, xpp.meta.get_class, xpp.meta.cross_reference, d365.docs.search]
arguments:
  - name: model
    description: Model to audit, e.g. "GTFin"
    required: true
---

Audit the **{{model}}** model against Dynamics 365 extensibility principles. In
F&O, over-layering standard objects is prohibited on cloud; customisations must
use **extensions**: Chain of Command (CoC), event handlers (pre/post), and
table/form/enum extensions.

1. **Inventory** — call `xpp.meta.get_model_contents` on `{{model}}`. For each member, note its kind (class, table, data entity, form, job, ...).
2. **Sample three objects** — pick the largest class, one table/extension, and one data entity. For each, fetch its source via `xpp.meta.get_class` / `xpp.meta.get_table` / `xpp.meta.get_data_entity` and check:
   a. Is it an **over-layer** of a standard object (same name as a Microsoft object) rather than an extension (`*_Extension`, augmenting a delegate, or a CoC wrapper)? Over-layering is a finding.
   b. Does it write directly to standard tables instead of going through a supported API / data entity?
   c. Does it subscribe to events / use CoC (`next` calls) where it modifies standard behaviour?
3. **Cross-reference cross-check** — for each standard-object touchpoint, call `xpp.meta.cross_reference` to see whether the dependency stays inside `{{model}}` or leaks into others.
4. **Extensibility procedure** — call `d365.docs.search` with `"extensibility Chain of Command event handler table extension"` to retrieve the canonical guidance.

Produce a 4-section report:
- **Extension compliance**: percentage of customisation touchpoints using extensions vs over-layering.
- **Direct standard-table writes**: count + worst offenders.
- **Extension-pattern usage**: CoC / event handlers / table extensions in use.
- **Recommended remediation**: ranked by effort vs benefit.

Do NOT propose code changes; produce only the audit report.
