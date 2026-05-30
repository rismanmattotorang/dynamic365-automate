---
name: d365.skill.data_entity_design
description: Design discipline for exposing a Dynamics 365 data entity (OData v4) as a stable, agent-friendly tool surface — metadata-first, EDM mapping, security-duty binding, read-only posture.
tags: [odata, data-entity, design, integration]
requires_tools: [d365.docs.search, d365.env.info, xpp.meta.get_data_entity]
arguments:
  - name: entity_name
    description: Data entity name (e.g. "PurchaseOrderHeadersV2")
    required: true
  - name: environment
    description: Target environment name OR base URL (e.g. "gt-prod" or "https://gt-prod.operations.dynamics.com")
    required: false
---

# Data entity design — Dynamics 365 OData

Turn a data entity name into a stable, agent-friendly tool surface.

**Target entity:** `{{entity_name}}`
**Environment:** `{{environment}}`

## Step 1 — Metadata first

Always inspect the entity metadata before tool design. Use
`xpp.meta.get_data_entity` (public collection name, properties, key) and/or the
live `$metadata`:

```
GET <base>/data/$metadata
Header: Accept: application/xml
```

Parse the `EntityType`, `EntitySet`, `NavigationProperty`, and bound `Action`
declarations. **Do not infer.** Cite the entity's documentation via
`d365.docs.search` and surface the `d365-entity://{{entity_name}}/structure` URI.

## Step 2 — Tool surface design

Map OData operations to MCP tool names following the convention
`<domain>.<verb>.<entity>`:

| OData operation | MCP tool name | Read-only? |
|---|---|---|
| `GET /data/<EntitySet>` (with `$filter`) | `<domain>.search.<entity_plural>` | yes |
| `GET /data/<EntitySet>(<key>)` | `<domain>.get.<entity_singular>` | yes |
| `GET .../<NavProp>` | `<domain>.list.<navprop>` | yes |
| `POST /data/<EntitySet>` | `<domain>.create.<entity_singular>` | **no — gate behind `--enable-writes`** |
| `PATCH .../(<key>)` | `<domain>.update.<entity_singular>` | **no** |
| `DELETE .../(<key>)` | `<domain>.delete.<entity_singular>` | **no** |
| bound `Action` | `<domain>.<action_lower>` | **no — actions always write** |

## Step 3 — Schema generation

Generate a JSON Schema from the EDM types:

| EDM type | JSON Schema |
|---|---|
| `Edm.String` (`MaxLength=N`) | `{"type":"string","maxLength":N}` |
| `Edm.Decimal` | `{"type":"string","pattern":"^-?\\d+\\.\\d+$"}` — strings, never floats, for amounts |
| `Edm.DateTimeOffset` | `{"type":"string","format":"date-time"}` |
| `Edm.Boolean` | `{"type":"boolean"}` |
| `Edm.Int64` | `{"type":"integer"}` |

Always include `dataAreaId` for company-scoped entities. Set
`"additionalProperties": false` so agents can't invent fields.

## Step 4 — Security binding

Every entity is gated by a security **duty/privilege**. Record the duty that
grants `Read` and the one that grants `Maintain`; the create/update/delete tools
must declare the maintain duty. Auth is **Microsoft Entra ID** OAuth2
client-credentials — never store credentials in the tool schema, never log them.

## Step 5 — Read-only safety posture

Default the new tool set to read-only; mark writes via the server's
`ExposurePolicy`:

```rust
ToolDescriptor::new("po.create.purchase_order", schema, handler)
    .with_writes()   // hidden from tools/list unless --enable-writes
```

High-stakes entities (ledger postings, customer master, deployments) must fire
the elicitation flow before the write.

## Step 6 — Verify

```
1. $metadata returns 200 with valid EDM XML        → integration test
2. tools/list emits one tool per entity operation  → cardinality unit test
3. search returns a non-empty result on the env     → integration test
4. every write tool hidden without --enable-writes  → exposure-policy test
5. every write fires elicitation before the POST    → round-trip test
```

Produce a markdown design doc with one section per step before coding.
