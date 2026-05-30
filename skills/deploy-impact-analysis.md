---
name: d365.skill.deploy_impact_analysis
description: Cross-domain impact analysis for a Dynamics 365 deployable package before deployment.
tags: [lifecycle, deployment, impact-analysis]
requires_tools: [xpp.meta.cross_reference, xpp.meta.get_model_contents, d365.entity.read, d365.docs.search]
arguments:
  - name: package
    description: Deployable package name, e.g. "GTFin-2026.04"
    required: true
  - name: target_environment
    description: Target environment (PRODUCTION / UAT / DEV)
    required: false
---

Analyse the impact of deploying package **{{package}}** to **{{target_environment}}** before deployment.

1. **Enumerate package contents** — call `xpp.meta.get_model_contents` for each model in the package to list every modified object (X++ class, table, data entity, form, job, etc.).
2. **Direct impact** — for each object, call `xpp.meta.cross_reference` to enumerate every caller, implementer, and reference site.
3. **Data-entity reach** — for any modified data entity, note which business areas read it (use `d365.entity.structure` to confirm company scoping and security duty).
4. **Business-process impact** — call `d365.docs.search` and `flow.find_process` with the model and module names to find which business processes the package touches.
5. **Pre-deploy checks** — call `d365.docs.search` with `"Solution Checker best practice database synchronize"` to retrieve the standard pre-deployment procedure.

Produce a 3-section report: *Direct impact*, *Indirect dependents*, *Recommended
pre-deploy checks* (Solution Checker, db sync, regression). Cite every claim. If
the impact crosses three or more models, recommend splitting the package.
