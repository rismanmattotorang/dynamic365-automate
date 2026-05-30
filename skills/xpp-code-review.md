---
name: d365.skill.xpp_code_review
description: Structured X++ code review with explicit checks for Dynamics 365-specific anti-patterns.
tags: [x++, review, quality]
requires_tools: [xpp.meta.get_class, xpp.meta.get_job, xpp.meta.cross_reference, d365.docs.search]
arguments:
  - name: object_name
    description: X++ object name to review, e.g. "GTFinJournalPoster"
    required: true
  - name: kind
    description: Object kind (class | job | table | data_entity)
    required: true
---

Review the **{{kind}}** **{{object_name}}** for Dynamics 365-specific code quality issues.

1. **Fetch source** — `xpp.meta.get_{{kind}}` with name={{object_name}} (use `xpp.meta.get_class` for classes/jobs).
2. **Static checks** (silent unless violations found):
   - **`delete_from` without a `where`** — full-table deletes are almost always a bug.
   - **Unbounded `while select`** — no range, no `firstOnly`, no `setTmp` capacity guard.
   - **Unbalanced `ttsBegin` / `ttsCommit`** — every transaction must be balanced; no early `return` inside a tts block.
   - **Direct write to a standard table** — should go through a supported API / data entity, not a raw `.insert()`/`.update()`.
   - **Over-layering instead of extension** — should be Chain of Command / event handler / table extension (see `d365.skill.extension_audit`).
   - **Hard-coded legal entity / currency / dimension values** — should be parameters or configuration.
   - **Business logic in `display` methods** — display methods run per-row in the UI; no DB work there.
3. **Architectural checks**:
   - Cross-reference (`xpp.meta.cross_reference`) — is this object referenced only inside its own model? If so it should not be `public`.
   - Any `runBuf` / direct SQL passthrough without parameter binding (injection risk)?
4. **Canon** — for any non-trivial pattern, call `d365.docs.search` with the relevant procedure name to confirm the Dynamics 365-canonical approach.

Produce a markdown review with severity tags (`error`, `warning`, `info`), each
citing the source location and the procedure URI. Do NOT modify the code.
