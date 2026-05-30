# Dynamics 365 Correctness Invariants

This document records the canonical sources backing every Dynamics 365 fixture
in the D365-Automate codebase. Drift from these sources is caught by the
precision tests in `crates/d365-automate-odata/src/client.rs` (module `tests`),
run as a dedicated CI job (`.github/workflows/ci.yml` → *Dynamics 365 correctness
invariants*).

When a fixture changes, either the source-of-truth changed (cite the new
Microsoft Learn page) or the change is a regression.

---

## Service catalogue (`seed_operations`)

Operations model OData v4 actions / Custom Service operations on a Finance &
Operations environment. Every operation declares its parameter signature,
read-only flag, change-set requirement, security, and a SAP→D365 mapping note.

| Operation | Service group | Read-only | `$batch` change set | Maps from (SAP) |
|---|---|---|---|---|
| `EnvironmentInfo` | Platform | yes | – | `RFC_SYSTEM_INFO` |
| `ReleasedProductGetDetail` | SupplyChain | yes | – | `BAPI_MATERIAL_GET_DETAIL` |
| `LedgerGeneralJournalEntryPost` | GeneralLedger | no | yes | `BAPI_ACC_DOCUMENT_POST` |
| `InventoryProductReceiptPost` | SupplyChain | no | yes | `BAPI_GOODSMVT_CREATE` |
| `PurchaseOrderCreate` | Procurement | no | yes | `BAPI_PO_CREATE1` |
| `SalesOrderCreate` | Sales | no | yes | `BAPI_SALESORDER_CREATEFROMDAT2` |
| `CustomerMaintain` | Sales | no | yes | customer master maintenance |

### The write contract

Where a SAP write BAPI must be followed by `BAPI_TRANSACTION_COMMIT`, a Dynamics
365 write is staged into an OData **`$batch` change set** and submitted as one
atomic unit of work — there is no auto-commit and no partial apply. Every write
operation therefore carries `uses_changeset: true` and returns an **Infolog**
status output. Enforced by:

- `every_write_operation_uses_changeset`
- `every_write_operation_returns_operation_status`

### Infolog / operation status

The Dynamics 365 analog of `BAPIRET2`. A response carries either an OData error
payload (`{ "error": { "code", "message" } }`) or an Infolog array of messages
with severities `Error` / `Warning` / `Info` (`Success` for clean operations).
`InfologSeverity::is_failure()` treats `Error` and unrecognised severities as
failures (fail-closed) so an unrecognised response never causes a silent commit.

### Security (`SecurityReference`)

Every operation references at least one security **privilege** (optionally inside
a **duty**) with an access level (`Read` / `Create` / `Update` / `Delete` /
`Execute`). This replaces SAP's `S_RFC` / `S_TABU_DIS` authorization rows.
Enforced by `every_operation_references_a_security_privilege`.

---

## Data-entity catalogue (`seed_entities`)

| Entity | Company-scoped | Key fields | Maps from (SAP) |
|---|---|---|---|
| `CompaniesV2` | no | `dataAreaId` | `T001` (company codes) |
| `FiscalCalendarPeriod` | yes | `dataAreaId`, `FiscalCalendarPeriodName` | `T001B` (posting periods) |
| `ReleasedProductsV2` | yes | `dataAreaId`, `ItemNumber` | `MARA` (material master) |
| `LedgerJournalTrans` | yes | `dataAreaId`, `JournalBatchNumber`, `LineNumber` | `BSEG` (accounting doc segment) |
| `GeneralJournalAccountEntry` | yes | `dataAreaId`, `GeneralJournalAccountEntryRecId` | `ACDOCA` / `FAGLFLEXA` (universal journal) |
| `CustomersV3` | yes | `dataAreaId`, `CustomerAccount` | customer master |
| `SalesOrderHeadersV2` | yes | `dataAreaId`, `SalesOrderNumber` | `VBAK` (sales doc header) |

### Company scoping

Where SAP scoped every table read by `MANDT`/`RCLNT` (the client, always the
first key), Dynamics 365 scopes company-specific data by **`DataAreaId`** (the
legal entity). Every company-scoped entity carries `dataAreaId` as a key, and
`read_entity` injects a `dataAreaId` filter when the caller omits one — so
cross-company leaks are impossible by construction. Enforced by
`every_company_scoped_entity_has_dataareaid_key`.

### Item identity

The released product is keyed by `ItemNumber` (logical type `ItemId`), the
Dynamics 365 replacement for SAP's `MATNR`. Enforced by
`item_number_follows_released_product_convention`.

### The universal journal

`GeneralJournalAccountEntry` is the single source of accounting truth — the
Dynamics 365 analog of SAP's `ACDOCA` universal journal. Enforced by
`general_journal_account_entry_is_the_subledger_truth`. Legacy SAP tables carry
a `legacy_mapping` note so migrating operators can find the D365 equivalent;
enforced by `legacy_tables_map_to_data_entities`.

---

## The seven invariants (CI gate)

```
every_write_operation_uses_changeset
every_write_operation_returns_operation_status
every_operation_references_a_security_privilege
every_company_scoped_entity_has_dataareaid_key
item_number_follows_released_product_convention
general_journal_account_entry_is_the_subledger_truth
legacy_tables_map_to_data_entities
```

These are the Dynamics 365 re-grounding of SAP-Automate's seven SAP-precision
tests; the crosswalk is in [`../PORTING.md`](../PORTING.md) §4.
