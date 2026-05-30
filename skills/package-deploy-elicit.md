---
name: d365.skill.package_deploy_elicit
description: Deployable-package deployment with a re-typed confirmation phrase and explicit opt-in for dangerous flags.
tags: [lifecycle, deployment, elicitation, workflow]
requires_tools: [d365.workflow.deploy_package, xpp.meta.cross_reference, d365.docs.search]
arguments:
  - name: package
    description: Deployable package name, e.g. "GTFin-2026.04"
    required: true
  - name: target_environment
    description: Target environment (DEV | UAT | PRODUCTION)
    required: false
---

Deploy package **{{package}}** to **{{target_environment}}**.

The `d365.workflow.deploy_package` tool elicits:

- **Package name** (pre-filled from the argument hint)
- **Target environment** (enum: DEV / UAT / PRODUCTION)
- **Run database synchronize?** (boolean; default true)
- **Skip Solution Checker?** (boolean; default false — `true` here is dangerous and the agent should warn the user)
- **Confirmation phrase** (the user must re-type the package name to proceed)

The tool refuses to execute if the confirmation phrase doesn't match the package
name, and refuses outright on clients that don't advertise the `elicitation`
capability — there is no way to silently deploy a package.

Pre-flight checklist before invoking the tool:

1. Call `xpp.meta.cross_reference` on the most critical objects in the package to surface unexpected impact (or run `d365.skill.deploy_impact_analysis`).
2. Call `d365.docs.search` with `"apply deployable package LCS"` to confirm the canonical procedure.
3. Call `d365.workflow.deploy_package` with the package hint.

Production deployments SHOULD NOT skip Solution Checker. If the user requests
`skip_solution_checker=true`, push back and ask the user to confirm in plain
text before submitting the elicitation form.
