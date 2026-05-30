---
name: d365.skill.security_sod_audit
description: Segregation-of-Duties (SoD) audit workflow for Dynamics 365 security — read-only analytical review across users, roles, duties, privileges, and critical entry points.
tags: [security, sod, audit, governance]
requires_tools: [d365.entity.read, d365.service.metadata, d365.docs.search, d365.env.info]
arguments:
  - name: user_or_role
    description: Target user ID OR security role name to audit (e.g. "edwin" or "System administrator")
    required: true
  - name: scope
    description: Audit scope — "user" (single user) | "role" (single role) | "system" (whole environment roll-up)
    required: true
---

# Segregation-of-Duties audit — read-only

This skill produces a structured audit report **without writing anything** — no
role assignments, no duty changes, no package creation.

**Target:** `{{user_or_role}}` (scope: `{{scope}}`)

Dynamics 365 security is hierarchical: **roles → duties → privileges → entry
points** (menu items / data entities / service operations). A user inherits the
union of the privileges of every assigned role.

## Step 1 — Identify the target

Call `d365.env.info` first. Record `environment` / `legal_entity` / `environment_role`.

For `{{scope}} == "user"`, read the user's role assignments:

```
d365.entity.read entity=SecurityUserRoleAssociations
  fields=UserId,SecurityRoleName,AssignmentStatus
  filters=["UserId eq '{{user_or_role}}'"]
```

For `{{scope}} == "role"`, read the role's duties:

```
d365.entity.read entity=SecurityRoleDutyAssociations
  fields=SecurityRoleName,SecurityDutyName
  filters=["SecurityRoleName eq '{{user_or_role}}'"]
```

## Step 2 — Enumerate the effective privileges

Walk role → duty → privilege so you have the flattened privilege set for the
target. Privileges name an access level (`Read` / `Update` / `Create` /
`Delete` / `Unset`) against an entry point.

## Step 3 — Apply the SoD rule library

Compare the effective entry-point set against canonical SoD conflict pairs:

| Conflict pair | Risk |
|---|---|
| Maintain vendor + Approve vendor invoice + Settle payment | Phantom-vendor fraud |
| Maintain customer + Create sales order + Post invoice | Phantom-customer revenue inflation |
| Create purchase order + Post product receipt + Match invoice | Three-way-match bypass |
| Maintain general journal + Post general journal (no approval) | Post unreviewed entries |
| Maintain user + Assign security role + Manage environment | Self-privilege escalation |

Cite the canonical guidance via `d365.docs.search` (search terms: "segregation of
duties", "security roles duties privileges", "sensitive duties").

## Step 4 — Critical privileges

Flag any of these maintain-level grants — they are universally over-privileged:

- `SystemAdministration` / `-SysAdmin-` role — full control
- `Maintain users` + `Maintain security roles` — user admin
- `Maintain database log` / `Manage data management` — bulk data export/import
- Any privilege granting `Delete` on a ledger or subledger entity

## Step 5 — Report shape

Produce a markdown report with these sections, in this order, and nothing else:

```markdown
# SoD Audit — {{user_or_role}}

## Target
- Environment: <environment> (<environment_role>), legal entity <legal_entity>
- Scope: {{scope}}
- Audit timestamp: <UTC ISO 8601>

## Findings
| Severity | Code | Title | Evidence |
|---|---|---|---|
| HIGH | SOD-001 | <conflict pair> | <duties/entry points that overlap> |

## Critical privileges
| Privilege | Access level | Entry point | Role |
|---|---|---|---|

## Citations
- d365-learn://... (security reference)
- d365-entity://SecurityUserRoleAssociations/structure

## Recommendation
- <Action 1 — a change-request title, never a direct write>
```

**Never** propose to write the fix yourself. SoD remediation requires a security
change request, governance approval, and a deployment — out of scope for the agent.
