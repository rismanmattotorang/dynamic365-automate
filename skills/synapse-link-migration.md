---
name: d365.skill.synapse_link_migration
description: Analytics modernisation workflow — inventory Dynamics 365 F&O reporting/analytics objects, classify migration patterns (lift-and-shift vs redesign), and produce a phased plan to Synapse Link for Dataverse / Microsoft Fabric.
tags: [analytics, synapse, fabric, modernization, planning]
requires_tools: [d365.docs.search, d365.entity.read, xpp.meta.cross_reference, kb.global_query]
arguments:
  - name: analytics_object
    description: Reporting object (Entity Store measurement / aggregate measurement / BYOD entity / SSRS report) OR "*" for a system-wide inventory
    required: true
  - name: target_platform
    description: Target platform (e.g. "Synapse Link for Dataverse", "Microsoft Fabric", "Power BI dataflows")
    required: false
---

# F&O analytics → Synapse Link / Fabric migration planning

Produce a **migration design document**, not a migration execution. All
operations are read-only. Legacy F&O analytics surfaces — the **Entity Store**
(measurements / aggregate measurements), **BYOD** (Bring Your Own Database export),
and embedded **SSRS** reports — are superseded by **Synapse Link for Dataverse**
and **Microsoft Fabric**.

**Target object:** `{{analytics_object}}`
**Target platform:** `{{target_platform}}`

## Step 1 — Object inventory

For `{{analytics_object}} == "*"` (system-wide), enumerate the major analytics
surfaces:

| Surface | Where to look | Notes |
|---|---|---|
| Aggregate measurements (Entity Store) | model metadata via `xpp.meta.get_model_contents` | deploy targets for Power BI |
| BYOD export entities | `DataManagementDefinitionGroups` / export projects | being retired in favour of Synapse Link |
| Data entities used as sources | `d365.entity.structure` | the canonical source for Synapse Link |
| SSRS reports | model metadata (`*Report` classes) | redesign as Power BI |

For a single object, fetch its definition and cross-references:

```
xpp.meta.cross_reference name=<object> kind=<class|data_entity>
```

## Step 2 — Classification matrix

| Legacy surface | Target counterpart | Effort |
|---|---|---|
| BYOD entity export | Synapse Link for Dataverse table | Low |
| Aggregate measurement | Fabric / Power BI semantic model over Synapse Link | Medium |
| Entity Store measurement | Redesign as Synapse Link + Fabric model | **High** |
| SSRS operational report | Power BI paginated report | Medium |
| SSRS with custom RDP class (X++) | **Redesign** (no direct equivalent) | **High** |

## Step 3 — Custom-code surfacing

Every Report Data Provider (RDP) class and BYOD post-processing routine is X++
that *will not run as-is* in Synapse Link / Fabric. Enumerate them via
`xpp.meta.cross_reference` and classify:

| Pattern | Target equivalent |
|---|---|
| Simple projection / join | Fabric SQL view / semantic model relationship |
| Currency / unit conversion | Fabric measure (DAX) |
| Hard-coded business rule | Manual rewrite as a Fabric notebook / dataflow |
| External call | Pipeline activity / Dataverse virtual table |

## Step 4 — Citation pass

Call `d365.docs.search` for the Synapse Link for Dataverse and Microsoft Fabric
guidance; cite both URIs. For large estates (>50 objects) also run
`kb.global_query query="analytics modernisation migration patterns"` for the
community-summary roll-up.

## Step 5 — Wave plan

```markdown
## Wave 1 — Foundation (weeks 0-4)
- Inventory + classification (this skill output)
- Enable Synapse Link for Dataverse; select source tables
- 3 reference tables proven end-to-end into Fabric

## Wave 2 — Active workloads (weeks 4-12)
- All high-criticality measurements → Fabric semantic models
- BYOD exports cut over to Synapse Link

## Wave 3 — Custom redesign (weeks 12-24)
- RDP-class reports → Power BI paginated / Fabric notebooks

## Wave 4 — Decommission (weeks 24-26)
- Disable BYOD export projects; archive stale reports
```

## Step 6 — Risk register

| ID | Risk | Mitigation |
|---|---|---|
| R1 | Near-real-time latency expectations vs Synapse Link cadence | Set SLOs; use Fabric direct-lake where needed |
| R2 | Reporting performance regression on first cut | Baseline P95 capture; semantic-model tuning |
| R3 | Security model differs (D365 duties vs Fabric workspace roles) | Run `d365.skill.security_sod_audit` before cut-over |
| R4 | Custom RDP routines have hidden side-effects | `xpp.meta.cross_reference` on every routine first |

**No write operations** are performed by this skill. The deliverable is a
markdown design document the platform team can turn into work items.
