//! `D365Client` trait and offline mock implementation.
//!
//! The trait is the central abstraction the MCP server depends on. Two
//! backends are envisioned:
//!   - `MockD365Client` — ships now; deterministic in-memory fixtures so the
//!     full MCP tool surface (environment info / service search / service
//!     metadata / service call / entity read / entity structure / bulk
//!     metadata) is callable offline and in CI.
//!   - `HttpD365Client` (Phase 3b): a live F&O OData v4 / Custom Service
//!     client over Microsoft Entra ID OAuth2, behind the same trait — no MCP
//!     server change required.
//!
//! Where SAP exposed RFC/BAPI function modules and DDIC tables, Dynamics 365
//! exposes **OData actions / Custom Service operations** and **data entities**.
//! The trait shape is preserved so the server tool layer ports with minimal
//! churn.

use crate::error::{D365Error, D365Result};
use crate::pool::ConnectionPool;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

// ===========================================================================
// Shared types
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    /// e.g. "gt-prod"
    pub environment: String,
    /// Default legal entity / DataAreaId, e.g. "USMF".
    pub legal_entity: String,
    /// e.g. "Dynamics 365 Finance 10.0.40"
    pub version: String,
    /// e.g. "PROD"
    pub environment_role: String,
    /// e.g. "https://gt-prod.operations.dynamics.com"
    pub base_url: String,
    /// e.g. "Finance & Operations"
    pub platform: String,
    /// `Credentials::redacted()`.
    pub identity: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ParamDirection { In, Out }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceParameter {
    pub name: String,
    pub direction: ParamDirection,
    /// EDM / OData type token (e.g. `Edm.String`, `Edm.Int64`,
    /// `Microsoft.Dynamics.DataEntities.LedgerJournalHeader`).
    #[serde(rename = "type")]
    pub type_token: String,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_value: Option<String>,
}

/// One Dynamics 365 security reference required to invoke an operation.
/// Replaces SAP's `S_RFC` authorization rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReference {
    /// Privilege name, e.g. `LedgerJournalEntryMaintain`.
    pub privilege: String,
    /// Containing duty, if any, e.g. `LedgerJournalEntryProcess`.
    #[serde(default)]
    pub duty: Option<String>,
    /// Access level: `Read` / `Create` / `Update` / `Delete` / `Execute`.
    pub access_level: String,
}

impl SecurityReference {
    pub fn read(privilege: &str) -> Self {
        Self { privilege: privilege.into(), duty: None, access_level: "Read".into() }
    }
    pub fn maintain(privilege: &str, duty: &str) -> Self {
        Self { privilege: privilege.into(), duty: Some(duty.into()), access_level: "Update".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceOperationMeta {
    /// OData action / Custom Service operation, e.g. `LedgerGeneralJournalEntryPost`.
    pub operation: String,
    pub description: String,
    /// Owning service group, e.g. `GeneralLedger` / `SupplyChain` / `Sales`.
    pub service_group: String,
    /// Model / package the operation belongs to, e.g. `GTFin`.
    #[serde(default)]
    pub model: Option<String>,
    pub parameters: Vec<ServiceParameter>,
    #[serde(default)]
    pub deprecated: bool,
    /// Whether the operation is safe to call read-only (read-only-by-default
    /// safety posture).
    pub read_only: bool,
    /// Whether the operation mutates state and therefore must be submitted
    /// inside an OData `$batch` change set (atomic unit of work). Replaces
    /// SAP's `commit_required`: Dynamics 365 never auto-commits a multi-step
    /// write — the caller stages operations and submits the change set.
    #[serde(default)]
    pub uses_changeset: bool,
    /// Security privileges/duties required to execute this operation.
    #[serde(default)]
    pub security: Vec<SecurityReference>,
    /// Dynamics 365-specific note (deprecations, entity supersessions, etc.).
    #[serde(default)]
    pub d365_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSummary {
    pub operation: String,
    pub description: String,
    pub service_group: String,
    pub read_only: bool,
    /// Rank score from the search; higher = better match.
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSearchResult {
    pub query: String,
    pub hits: Vec<ServiceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCallRequest {
    pub operation: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// If true, the call is rejected when the client is in read-only mode AND
    /// the operation is not declared `read_only` in its metadata.
    #[serde(default = "default_true")]
    pub require_read_only_safe: bool,
}

fn default_timeout_ms() -> u64 { 30_000 }
fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkMetadata {
    pub language: String,
    pub operations: Vec<ServiceOperationMeta>,
    /// Operations that were requested but not found.
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityField {
    pub name: String,
    /// EDM type, e.g. `Edm.String`, `Edm.Decimal`, `Edm.DateTimeOffset`.
    pub edm_type: String,
    /// Logical Dynamics 365 type (e.g. `ItemId`, `DataAreaId`, `AmountMST`).
    #[serde(rename = "type")]
    pub type_token: String,
    pub length: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityStructure {
    pub entity: String,
    pub description: String,
    pub fields: Vec<EntityField>,
    pub key_fields: Vec<String>,
    /// Security duty/privilege that gates this entity (replaces SAP's
    /// `authorization_group` / `S_TABU_DIS`). Empty for unrestricted entities.
    #[serde(default)]
    pub security: String,
    /// Whether the entity is scoped by legal entity (`DataAreaId`). Replaces
    /// SAP's MANDT/RCLNT client-first-key convention.
    #[serde(default)]
    pub company_scoped: bool,
    /// Mapping note for operators migrating from SAP (e.g. which DDIC table
    /// this data entity is the Dynamics 365 analog of). Empty when N/A.
    #[serde(default)]
    pub legacy_mapping: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadEntityRequest {
    pub entity: String,
    /// `$select` projection; empty = all fields.
    #[serde(default)]
    pub fields: Vec<String>,
    /// `$filter` clauses (`Field eq 'value'` / `Field like 'pattern'`).
    #[serde(default)]
    pub filters: Vec<String>,
    /// `$top`. Defaults to 100; refuses more than 1000 (buffer-overflow safety).
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
}

fn default_max_rows() -> usize { 100 }

pub const MAX_ROWS_HARD_CAP: usize = 1000;

/// The OData property name that scopes a record to a legal entity.
pub const COMPANY_FIELD: &str = "dataAreaId";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRow {
    pub values: serde_json::Map<String, serde_json::Value>,
}

// ===========================================================================
// D365Client trait
// ===========================================================================

#[async_trait]
pub trait D365Client: Send + Sync {
    async fn environment_info(&self) -> D365Result<EnvironmentInfo>;

    async fn search_service(&self, query: &str, limit: usize) -> D365Result<ServiceSearchResult>;

    async fn service_metadata(&self, operation: &str, language: &str) -> D365Result<ServiceOperationMeta>;

    async fn bulk_service_metadata(&self, operations: &[String], language: &str) -> D365Result<BulkMetadata>;

    async fn call_service(&self, request: ServiceCallRequest, read_only_mode: bool) -> D365Result<serde_json::Value>;

    async fn read_entity(&self, request: ReadEntityRequest) -> D365Result<Vec<EntityRow>>;

    async fn entity_structure(&self, entity: &str) -> D365Result<EntityStructure>;

    /// Pool snapshot for the TUI / Prometheus dashboards.
    fn pool_status(&self) -> PoolStatus {
        PoolStatus { cap: 0, available: 0 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PoolStatus { pub cap: usize, pub available: usize }

// ===========================================================================
// MockD365Client — offline reference implementation
// ===========================================================================

/// Mock client backed by realistic Dynamics 365-shaped fixtures spanning
/// General Ledger, Supply Chain, and Sales. Lets the MCP server be exercised
/// end-to-end without a live environment.
pub struct MockD365Client {
    pool: ConnectionPool,
    operations: HashMap<String, ServiceOperationMeta>,
    entities: HashMap<String, MockEntity>,
    identity: serde_json::Value,
    legal_entity: String,
}

pub(crate) struct MockEntity {
    structure: EntityStructure,
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
}

impl MockD365Client {
    pub fn new(pool_size: usize, identity: serde_json::Value) -> Arc<Self> {
        let legal_entity = identity.get("legal_entity")
            .and_then(|v| v.as_str()).unwrap_or("USMF").to_string();
        let mut s = Self {
            pool: ConnectionPool::new(pool_size),
            operations: HashMap::new(),
            entities: HashMap::new(),
            identity,
            legal_entity,
        };
        for op in seed_operations() { s.operations.insert(op.operation.clone(), op); }
        for e in seed_entities() { s.entities.insert(e.structure.entity.clone(), e); }
        Arc::new(s)
    }
}

#[async_trait]
impl D365Client for MockD365Client {
    async fn environment_info(&self) -> D365Result<EnvironmentInfo> {
        let _p = self.pool.acquire().await?;
        Ok(EnvironmentInfo {
            environment: "gt-dev".into(),
            legal_entity: self.legal_entity.clone(),
            version: "Dynamics 365 Finance 10.0.40 (mock)".into(),
            environment_role: "DEV".into(),
            base_url: "https://gt-dev.operations.dynamics.com".into(),
            platform: "Finance & Operations".into(),
            identity: self.identity.clone(),
        })
    }

    async fn search_service(&self, query: &str, limit: usize) -> D365Result<ServiceSearchResult> {
        let _p = self.pool.acquire().await?;
        let q = query.to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        let mut hits: Vec<ServiceSummary> = self.operations.values()
            .filter_map(|f| {
                let hay = format!("{} {} {}", f.operation.to_lowercase(), f.description.to_lowercase(), f.service_group.to_lowercase());
                let score: usize = terms.iter().map(|t| hay.matches(t).count()).sum();
                if score == 0 { None }
                else {
                    Some(ServiceSummary {
                        operation: f.operation.clone(),
                        description: f.description.clone(),
                        service_group: f.service_group.clone(),
                        read_only: f.read_only,
                        score: score as f32,
                    })
                }
            })
            .collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(limit.max(1));
        Ok(ServiceSearchResult { query: query.into(), hits })
    }

    async fn service_metadata(&self, operation: &str, _language: &str) -> D365Result<ServiceOperationMeta> {
        let _p = self.pool.acquire().await?;
        self.operations.get(operation)
            .cloned()
            .ok_or_else(|| D365Error::NotFound(operation.into()))
    }

    async fn bulk_service_metadata(&self, operations: &[String], language: &str) -> D365Result<BulkMetadata> {
        let _p = self.pool.acquire().await?;
        let mut out = Vec::new();
        let mut missing = Vec::new();
        for f in operations {
            match self.operations.get(f) {
                Some(meta) => out.push(meta.clone()),
                None => missing.push(f.clone()),
            }
        }
        Ok(BulkMetadata { language: language.into(), operations: out, missing })
    }

    async fn call_service(&self, request: ServiceCallRequest, read_only_mode: bool) -> D365Result<serde_json::Value> {
        let _p = self.pool.acquire().await?;
        let meta = self.operations.get(&request.operation)
            .ok_or_else(|| D365Error::NotFound(request.operation.clone()))?;
        if read_only_mode && !meta.read_only {
            return Err(D365Error::PermissionDenied(format!(
                "operation '{}' modifies state; not callable in read-only mode",
                request.operation,
            )));
        }

        let args = match &request.parameters {
            serde_json::Value::Object(m) => m.clone(),
            serde_json::Value::Null => serde_json::Map::new(),
            other => return Err(D365Error::InvalidParameter {
                name: "parameters".into(),
                reason: format!("expected object, got {}", other),
            }),
        };
        for p in &meta.parameters {
            if p.direction == ParamDirection::In && !p.optional && !args.contains_key(&p.name) {
                return Err(D365Error::InvalidParameter {
                    name: p.name.clone(),
                    reason: "required input parameter missing".into(),
                });
            }
        }

        debug!(operation = %request.operation, "mock OData operation executed");
        Ok(serde_json::json!({
            "operation": request.operation,
            "executed_on": "gt-dev",
            "inputs": args,
            "outputs": mock_outputs(meta, &args),
        }))
    }

    async fn read_entity(&self, request: ReadEntityRequest) -> D365Result<Vec<EntityRow>> {
        let _p = self.pool.acquire().await?;
        if request.max_rows == 0 {
            return Err(D365Error::InvalidParameter {
                name: "max_rows".into(),
                reason: "must be >= 1".into(),
            });
        }
        if request.max_rows > MAX_ROWS_HARD_CAP {
            return Err(D365Error::QueryResultOverflow {
                entity: request.entity.clone(),
                max_rows: request.max_rows,
            });
        }
        let entity = self.entities.get(&request.entity)
            .ok_or_else(|| D365Error::NotFound(request.entity.clone()))?;

        let projection: Vec<String> = if request.fields.is_empty() {
            entity.structure.fields.iter().map(|f| f.name.clone()).collect()
        } else {
            for f in &request.fields {
                if !entity.structure.fields.iter().any(|tf| tf.name.eq_ignore_ascii_case(f)) {
                    return Err(D365Error::InvalidParameter {
                        name: "fields".into(),
                        reason: format!("unknown field '{f}'"),
                    });
                }
            }
            request.fields.clone()
        };

        let mut conditions = parse_conditions(&request.filters)?;

        // Dynamics 365 queries are legal-entity scoped. If the caller didn't
        // specify a `dataAreaId` filter and the entity is company-scoped,
        // restrict to the connection's legal entity so cross-company leaks
        // are impossible by construction.
        if entity.structure.company_scoped {
            let has_company_filter = conditions.iter()
                .any(|(f, _, _)| f.eq_ignore_ascii_case(COMPANY_FIELD));
            if !has_company_filter {
                conditions.push((COMPANY_FIELD.into(), "=".into(), self.legal_entity.clone()));
            }
        }

        let mut rows: Vec<EntityRow> = Vec::new();
        for row in &entity.rows {
            if conditions.iter().all(|(field, op, value)| match_row(row, field, op, value)) {
                let projected: serde_json::Map<String, serde_json::Value> = projection.iter()
                    .filter_map(|f| row.iter().find(|(k, _)| k.eq_ignore_ascii_case(f)).map(|(k, v)| (k.clone(), v.clone())))
                    .collect();
                rows.push(EntityRow { values: projected });
                if rows.len() >= request.max_rows { break; }
            }
        }
        Ok(rows)
    }

    async fn entity_structure(&self, entity: &str) -> D365Result<EntityStructure> {
        let _p = self.pool.acquire().await?;
        self.entities.get(entity)
            .map(|t| t.structure.clone())
            .ok_or_else(|| D365Error::NotFound(entity.into()))
    }

    fn pool_status(&self) -> PoolStatus {
        PoolStatus { cap: self.pool.cap(), available: self.pool.available() }
    }
}

fn mock_outputs(meta: &ServiceOperationMeta, _args: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for p in &meta.parameters {
        if p.direction == ParamDirection::Out {
            out.insert(p.name.clone(), serde_json::Value::String(format!("<mock {}>", p.type_token)));
        }
    }
    serde_json::Value::Object(out)
}

/// Parse "Field eq 'value'" / "Field like 'pattern'" (OData) or
/// "FIELD = 'value'" into (field, op, value).
fn parse_conditions(raw: &[String]) -> D365Result<Vec<(String, String, String)>> {
    let mut out = Vec::new();
    for s in raw {
        let trimmed = s.trim();
        let upper = trimmed.to_uppercase();
        let (field, op, val) = if let Some(idx) = upper.find(" LIKE ") {
            (trimmed[..idx].trim().to_string(), "LIKE".to_string(), strip_val(&trimmed[idx + 6..]))
        } else if let Some(idx) = upper.find(" EQ ") {
            (trimmed[..idx].trim().to_string(), "=".to_string(), strip_val(&trimmed[idx + 4..]))
        } else if let Some(idx) = trimmed.find('=') {
            (trimmed[..idx].trim().to_string(), "=".to_string(), strip_val(&trimmed[idx + 1..]))
        } else {
            return Err(D365Error::InvalidParameter {
                name: "filters".into(),
                reason: format!("unsupported clause '{s}' (expected Field eq 'value' or Field like 'pattern')"),
            });
        };
        out.push((field, op, val));
    }
    Ok(out)
}

fn strip_val(s: &str) -> String {
    s.trim().trim_matches('\'').to_string()
}

fn match_row(row: &serde_json::Map<String, serde_json::Value>, field: &str, op: &str, value: &str) -> bool {
    let actual = row.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(field))
        .map(|(_, v)| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    match op {
        "=" => actual.eq_ignore_ascii_case(value),
        "LIKE" => sql_like(&actual, value),
        _ => false,
    }
}

fn sql_like(haystack: &str, pattern: &str) -> bool {
    let h = haystack.to_lowercase();
    let p = pattern.to_lowercase();
    if !p.contains('%') && !p.contains('_') { return h == p; }
    let stripped = p.replace('_', "");
    if let Some(rest) = stripped.strip_prefix('%') {
        let rest = rest.strip_suffix('%').unwrap_or(rest);
        h.contains(rest)
    } else if let Some(prefix) = stripped.strip_suffix('%') {
        h.starts_with(prefix)
    } else {
        h == stripped
    }
}

// ===========================================================================
// Fixtures
// ===========================================================================
//
// Dynamics 365 signature constants — sourced from the F&O OData metadata and
// Microsoft Learn. Every write operation carries `uses_changeset: true`
// because Dynamics 365 never auto-commits a multi-step write; the caller
// stages operations into an OData `$batch` change set and submits atomically.

fn p_in(name: &str, ty: &str, opt: bool, desc: &str) -> ServiceParameter {
    ServiceParameter { name: name.into(), direction: ParamDirection::In, type_token: ty.into(), optional: opt, description: if desc.is_empty() { None } else { Some(desc.into()) }, default_value: None }
}
fn p_out(name: &str, ty: &str, desc: &str) -> ServiceParameter {
    ServiceParameter { name: name.into(), direction: ParamDirection::Out, type_token: ty.into(), optional: false, description: if desc.is_empty() { None } else { Some(desc.into()) }, default_value: None }
}

pub(crate) fn seed_operations() -> Vec<ServiceOperationMeta> {
    vec![
        // ---- Diagnostics --------------------------------------------------
        ServiceOperationMeta {
            operation: "EnvironmentInfo".into(),
            description: "Retrieve environment identity (environment, legal entity, version, base URL).".into(),
            service_group: "Platform".into(),
            model: Some("ApplicationPlatform".into()),
            parameters: vec![ p_out("Info", "Edm.String", "Environment identity payload") ],
            deprecated: false, read_only: true, uses_changeset: false,
            security: vec![ SecurityReference::read("SystemAdministrationView") ],
            d365_notes: None,
        },
        // ---- Supply Chain: read released product -------------------------
        ServiceOperationMeta {
            operation: "ReleasedProductGetDetail".into(),
            description: "Read released product (item) master detail.  Read-only.".into(),
            service_group: "SupplyChain".into(),
            model: Some("ApplicationSuite".into()),
            parameters: vec![
                p_in("ItemNumber", "Edm.String", false, "Released product item number"),
                p_in("dataAreaId", "Edm.String", true, "Legal entity (defaults to connection's)"),
                p_out("Product", "Microsoft.Dynamics.DataEntities.ReleasedProductsV2", "Released product entity"),
            ],
            deprecated: false, read_only: true, uses_changeset: false,
            security: vec![ SecurityReference::read("EcoResProductView") ],
            d365_notes: Some("The SAP material master (MARA / BAPI_MATERIAL_GET_DETAIL) maps to the ReleasedProductsV2 data entity; the item number replaces MATNR.".into()),
        },
        // ---- General Ledger: post journal --------------------------------
        ServiceOperationMeta {
            operation: "LedgerGeneralJournalEntryPost".into(),
            description: "Post a general ledger journal entry.  Submitted inside a $batch change set; never auto-commits.".into(),
            service_group: "GeneralLedger".into(),
            model: Some("GTFin".into()),
            parameters: vec![
                p_in("JournalBatchNumber", "Edm.String", false, "Ledger journal header batch number"),
                p_in("JournalLines", "Collection(Microsoft.Dynamics.DataEntities.LedgerJournalTrans)", false, "Journal lines"),
                p_out("Voucher", "Edm.String", "Resulting voucher number"),
                p_out("Infolog", "Collection(Edm.String)", "Operation status messages"),
            ],
            deprecated: false, read_only: false, uses_changeset: true,
            security: vec![ SecurityReference::maintain("LedgerJournalEntryMaintain", "LedgerJournalEntryProcess") ],
            d365_notes: Some("The SAP accounting document post (BAPI_ACC_DOCUMENT_POST → ACDOCA) maps to a ledger journal post writing GeneralJournalAccountEntry.".into()),
        },
        // ---- Supply Chain: post product receipt --------------------------
        ServiceOperationMeta {
            operation: "InventoryProductReceiptPost".into(),
            description: "Post a product (goods) receipt against a purchase order.".into(),
            service_group: "SupplyChain".into(),
            model: Some("ApplicationSuite".into()),
            parameters: vec![
                p_in("PurchaseOrderNumber", "Edm.String", false, "Purchase order number"),
                p_in("ProductReceiptNumber", "Edm.String", false, "Vendor product receipt number"),
                p_out("Voucher", "Edm.String", "Resulting inventory voucher"),
                p_out("Infolog", "Collection(Edm.String)", "Operation status messages"),
            ],
            deprecated: false, read_only: false, uses_changeset: true,
            security: vec![ SecurityReference::maintain("VendProductReceiptMaintain", "VendProductReceiptProcess") ],
            d365_notes: Some("Maps from the SAP goods movement post (BAPI_GOODSMVT_CREATE).".into()),
        },
        // ---- Procurement: create purchase order --------------------------
        ServiceOperationMeta {
            operation: "PurchaseOrderCreate".into(),
            description: "Create a purchase order (header + lines).  Submitted inside a $batch change set.".into(),
            service_group: "Procurement".into(),
            model: Some("ApplicationSuite".into()),
            parameters: vec![
                p_in("OrderAccount", "Edm.String", false, "Vendor account"),
                p_in("dataAreaId", "Edm.String", false, "Legal entity"),
                p_in("PurchaseOrderLines", "Collection(Microsoft.Dynamics.DataEntities.PurchaseOrderLinesV2)", false, "Order lines"),
                p_out("PurchaseOrderNumber", "Edm.String", "Resulting purchase order number"),
                p_out("Infolog", "Collection(Edm.String)", "Operation status messages"),
            ],
            deprecated: false, read_only: false, uses_changeset: true,
            security: vec![ SecurityReference::maintain("PurchPurchaseOrderMaintain", "PurchPurchaseOrderProcess") ],
            d365_notes: Some("Maps from the SAP purchase order create (BAPI_PO_CREATE1).".into()),
        },
        // ---- Sales: create sales order -----------------------------------
        ServiceOperationMeta {
            operation: "SalesOrderCreate".into(),
            description: "Create a sales order (header + lines).  Submitted inside a $batch change set.".into(),
            service_group: "Sales".into(),
            model: Some("ApplicationSuite".into()),
            parameters: vec![
                p_in("CustomerAccount", "Edm.String", false, "Customer account"),
                p_in("dataAreaId", "Edm.String", false, "Legal entity"),
                p_in("SalesOrderLines", "Collection(Microsoft.Dynamics.DataEntities.SalesOrderLinesV2)", false, "Order lines"),
                p_out("SalesOrderNumber", "Edm.String", "Resulting sales order number"),
                p_out("Infolog", "Collection(Edm.String)", "Operation status messages"),
            ],
            deprecated: false, read_only: false, uses_changeset: true,
            security: vec![ SecurityReference::maintain("SalesSalesOrderMaintain", "SalesOrderProcess") ],
            d365_notes: Some("Maps from the SAP sales order create (BAPI_SALESORDER_CREATEFROMDAT2).".into()),
        },
        // ---- Sales: maintain customer ------------------------------------
        ServiceOperationMeta {
            operation: "CustomerMaintain".into(),
            description: "Create or update a customer master record.  Submitted inside a $batch change set.".into(),
            service_group: "Sales".into(),
            model: Some("ApplicationSuite".into()),
            parameters: vec![
                p_in("CustomerAccount", "Edm.String", true, "Customer account (omit to create)"),
                p_in("dataAreaId", "Edm.String", false, "Legal entity"),
                p_in("OrganizationName", "Edm.String", false, "Customer organization name"),
                p_out("CustomerAccount", "Edm.String", "Resulting customer account"),
                p_out("Infolog", "Collection(Edm.String)", "Operation status messages"),
            ],
            deprecated: false, read_only: false, uses_changeset: true,
            security: vec![ SecurityReference::maintain("CustCustomerMaintain", "CustCustomerMasterProcess") ],
            d365_notes: Some("Maps from the SAP customer master maintenance; the customer lives in CustomersV3 (Business Partner unification analog).".into()),
        },
    ]
}

fn field(name: &str, edm: &str, ty: &str, len: u32, key: bool, desc: &str) -> EntityField {
    EntityField { name: name.into(), edm_type: edm.into(), type_token: ty.into(), length: len, description: if desc.is_empty() { None } else { Some(desc.into()) }, key }
}

fn row(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

pub(crate) fn seed_entities() -> Vec<MockEntity> {
    use serde_json::json;
    vec![
        // CompaniesV2 — legal entities. Keyed by DataAreaId.
        MockEntity {
            structure: EntityStructure {
                entity: "CompaniesV2".into(),
                description: "Legal entities / companies.".into(),
                fields: vec![
                    field("dataAreaId", "Edm.String", "DataAreaId", 4, true, "Legal entity id"),
                    field("CompanyName", "Edm.String", "Name", 60, false, "Legal entity name"),
                    field("CurrencyCode", "Edm.String", "CurrencyCode", 3, false, "Accounting currency"),
                ],
                key_fields: vec!["dataAreaId".into()],
                security: "CompanyView".into(),
                company_scoped: false,
                legacy_mapping: Some("SAP T001 (company codes).".into()),
            },
            rows: vec![
                row(&[("dataAreaId", json!("USMF")), ("CompanyName", json!("Contoso Entertainment US")), ("CurrencyCode", json!("USD"))]),
                row(&[("dataAreaId", json!("GBSI")), ("CompanyName", json!("Contoso UK")), ("CurrencyCode", json!("GBP"))]),
            ],
        },
        // FiscalCalendarPeriod — ledger periods.
        MockEntity {
            structure: EntityStructure {
                entity: "FiscalCalendarPeriod".into(),
                description: "Ledger fiscal calendar periods.".into(),
                fields: vec![
                    field("dataAreaId", "Edm.String", "DataAreaId", 4, true, "Legal entity id"),
                    field("FiscalCalendarPeriodName", "Edm.String", "Name", 30, true, "Period name"),
                    field("Status", "Edm.String", "PeriodStatus", 12, false, "Open / OnHold / Closed"),
                ],
                key_fields: vec!["dataAreaId".into(), "FiscalCalendarPeriodName".into()],
                security: "LedgerPeriodView".into(),
                company_scoped: true,
                legacy_mapping: Some("SAP T001B (posting periods).".into()),
            },
            rows: vec![
                row(&[("dataAreaId", json!("USMF")), ("FiscalCalendarPeriodName", json!("2026-M03")), ("Status", json!("Closed"))]),
                row(&[("dataAreaId", json!("USMF")), ("FiscalCalendarPeriodName", json!("2026-M04")), ("Status", json!("Open"))]),
            ],
        },
        // ReleasedProductsV2 — item master.
        MockEntity {
            structure: EntityStructure {
                entity: "ReleasedProductsV2".into(),
                description: "Released products (item master).".into(),
                fields: vec![
                    field("dataAreaId", "Edm.String", "DataAreaId", 4, true, "Legal entity id"),
                    field("ItemNumber", "Edm.String", "ItemId", 20, true, "Released product item number"),
                    field("ProductName", "Edm.String", "Name", 60, false, "Product name"),
                    field("ItemModelGroupId", "Edm.String", "ItemModelGroupId", 10, false, "Item model group"),
                ],
                key_fields: vec!["dataAreaId".into(), "ItemNumber".into()],
                security: "EcoResProductView".into(),
                company_scoped: true,
                legacy_mapping: Some("SAP MARA (general material data); ItemNumber replaces MATNR.".into()),
            },
            rows: vec![
                row(&[("dataAreaId", json!("USMF")), ("ItemNumber", json!("D0001")), ("ProductName", json!("Mid-Range Speaker")), ("ItemModelGroupId", json!("FIFO"))]),
            ],
        },
        // LedgerJournalTrans — general journal lines.
        MockEntity {
            structure: EntityStructure {
                entity: "LedgerJournalTrans".into(),
                description: "General journal lines.".into(),
                fields: vec![
                    field("dataAreaId", "Edm.String", "DataAreaId", 4, true, "Legal entity id"),
                    field("JournalBatchNumber", "Edm.String", "JournalNum", 20, true, "Journal header batch number"),
                    field("LineNumber", "Edm.Int64", "LineNum", 0, true, "Line number"),
                    field("AccountDisplayValue", "Edm.String", "LedgerDimension", 40, false, "Main account + dimensions"),
                    field("AmountCurrencyDebit", "Edm.Decimal", "AmountCurDebit", 0, false, "Debit amount"),
                    field("AmountCurrencyCredit", "Edm.Decimal", "AmountCurCredit", 0, false, "Credit amount"),
                ],
                key_fields: vec!["dataAreaId".into(), "JournalBatchNumber".into(), "LineNumber".into()],
                security: "LedgerJournalView".into(),
                company_scoped: true,
                legacy_mapping: Some("SAP BSEG (accounting document segment).".into()),
            },
            rows: vec![
                row(&[("dataAreaId", json!("USMF")), ("JournalBatchNumber", json!("000123")), ("LineNumber", json!(1)), ("AccountDisplayValue", json!("110180")), ("AmountCurrencyDebit", json!("1000.00")), ("AmountCurrencyCredit", json!("0.00"))]),
            ],
        },
        // GeneralJournalAccountEntry — the universal accounting truth.
        MockEntity {
            structure: EntityStructure {
                entity: "GeneralJournalAccountEntry".into(),
                description: "Subledger general journal account entries — the universal accounting truth.".into(),
                fields: vec![
                    field("dataAreaId", "Edm.String", "DataAreaId", 4, true, "Legal entity id"),
                    field("GeneralJournalAccountEntryRecId", "Edm.Int64", "RecId", 0, true, "Record id"),
                    field("MainAccount", "Edm.String", "MainAccount", 25, false, "Main account"),
                    field("AccountingCurrencyAmount", "Edm.Decimal", "AmountMST", 0, false, "Accounting currency amount"),
                    field("PostingType", "Edm.String", "PostingType", 30, false, "Posting type"),
                ],
                key_fields: vec!["dataAreaId".into(), "GeneralJournalAccountEntryRecId".into()],
                security: "LedgerTransView".into(),
                company_scoped: true,
                legacy_mapping: Some("SAP ACDOCA / FAGLFLEXA (universal journal); this is the Dynamics 365 single source of accounting truth.".into()),
            },
            rows: vec![
                row(&[("dataAreaId", json!("USMF")), ("GeneralJournalAccountEntryRecId", json!(5637144576i64)), ("MainAccount", json!("110180")), ("AccountingCurrencyAmount", json!("1000.00")), ("PostingType", json!("LedgerJournal"))]),
            ],
        },
        // CustomersV3 — customer master.
        MockEntity {
            structure: EntityStructure {
                entity: "CustomersV3".into(),
                description: "Customer master records.".into(),
                fields: vec![
                    field("dataAreaId", "Edm.String", "DataAreaId", 4, true, "Legal entity id"),
                    field("CustomerAccount", "Edm.String", "AccountNum", 20, true, "Customer account"),
                    field("OrganizationName", "Edm.String", "Name", 100, false, "Customer name"),
                    field("CustomerGroupId", "Edm.String", "CustGroup", 10, false, "Customer group"),
                ],
                key_fields: vec!["dataAreaId".into(), "CustomerAccount".into()],
                security: "CustCustomerView".into(),
                company_scoped: true,
                legacy_mapping: Some("SAP customer master (Business Partner unification analog).".into()),
            },
            rows: vec![
                row(&[("dataAreaId", json!("USMF")), ("CustomerAccount", json!("US-001")), ("OrganizationName", json!("Forest Wholesales")), ("CustomerGroupId", json!("10"))]),
            ],
        },
        // SalesOrderHeadersV2 — sales order headers.
        MockEntity {
            structure: EntityStructure {
                entity: "SalesOrderHeadersV2".into(),
                description: "Sales order headers.".into(),
                fields: vec![
                    field("dataAreaId", "Edm.String", "DataAreaId", 4, true, "Legal entity id"),
                    field("SalesOrderNumber", "Edm.String", "SalesId", 20, true, "Sales order number"),
                    field("OrderingCustomerAccountNumber", "Edm.String", "CustAccount", 20, false, "Customer account"),
                    field("SalesOrderStatus", "Edm.String", "SalesStatus", 20, false, "Order status"),
                ],
                key_fields: vec!["dataAreaId".into(), "SalesOrderNumber".into()],
                security: "SalesOrderView".into(),
                company_scoped: true,
                legacy_mapping: Some("SAP VBAK (sales document header).".into()),
            },
            rows: vec![
                row(&[("dataAreaId", json!("USMF")), ("SalesOrderNumber", json!("SO-000178")), ("OrderingCustomerAccountNumber", json!("US-001")), ("SalesOrderStatus", json!("Backorder"))]),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn identity() -> serde_json::Value { json!({ "legal_entity": "USMF" }) }

    #[tokio::test]
    async fn environment_info_reports_legal_entity() {
        let c = MockD365Client::new(2, identity());
        let info = c.environment_info().await.unwrap();
        assert_eq!(info.legal_entity, "USMF");
        assert_eq!(info.platform, "Finance & Operations");
    }

    #[tokio::test]
    async fn search_finds_ledger_post() {
        let c = MockD365Client::new(2, identity());
        let res = c.search_service("ledger journal post", 5).await.unwrap();
        assert!(res.hits.iter().any(|h| h.operation == "LedgerGeneralJournalEntryPost"));
    }

    #[tokio::test]
    async fn read_entity_injects_company_scope() {
        let c = MockD365Client::new(2, identity());
        // No filter given; company-scoped entity must restrict to USMF only.
        let rows = c.read_entity(ReadEntityRequest {
            entity: "FiscalCalendarPeriod".into(),
            fields: vec![],
            filters: vec![],
            max_rows: 100,
        }).await.unwrap();
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| r.values.get("dataAreaId").and_then(|v| v.as_str()) == Some("USMF")));
    }

    #[tokio::test]
    async fn read_only_mode_blocks_write_operation() {
        let c = MockD365Client::new(2, identity());
        let err = c.call_service(ServiceCallRequest {
            operation: "LedgerGeneralJournalEntryPost".into(),
            parameters: json!({ "JournalBatchNumber": "000123", "JournalLines": [] }),
            timeout_ms: 1000,
            require_read_only_safe: true,
        }, true).await.unwrap_err();
        assert!(matches!(err, D365Error::PermissionDenied(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn over_cap_read_is_query_overflow() {
        let c = MockD365Client::new(2, identity());
        let err = c.read_entity(ReadEntityRequest {
            entity: "CompaniesV2".into(), fields: vec![], filters: vec![], max_rows: 5000,
        }).await.unwrap_err();
        assert!(matches!(err, D365Error::QueryResultOverflow { .. }), "got {err:?}");
    }

    // ---- Dynamics 365 correctness invariants (ported from SAP_CORRECTNESS) ----

    #[test]
    fn every_write_operation_uses_changeset() {
        for op in seed_operations() {
            if !op.read_only {
                assert!(op.uses_changeset,
                    "write operation '{}' must be submitted inside a $batch change set", op.operation);
            }
        }
    }

    #[test]
    fn every_write_operation_returns_operation_status() {
        // The Dynamics 365 analog of "every write BAPI returns BAPIRET2":
        // every write operation must surface an Infolog / status output.
        for op in seed_operations() {
            if !op.read_only {
                let has_status = op.parameters.iter().any(|p|
                    p.direction == ParamDirection::Out && p.name.eq_ignore_ascii_case("Infolog"));
                assert!(has_status,
                    "write operation '{}' must return an Infolog status output", op.operation);
            }
        }
    }

    #[test]
    fn every_operation_references_a_security_privilege() {
        for op in seed_operations() {
            assert!(!op.security.is_empty(),
                "operation '{}' must declare at least one security privilege", op.operation);
        }
    }

    #[test]
    fn every_company_scoped_entity_has_dataareaid_key() {
        for e in seed_entities() {
            if e.structure.company_scoped {
                assert!(e.structure.key_fields.iter().any(|k| k == COMPANY_FIELD),
                    "company-scoped entity '{}' must carry dataAreaId as a key field", e.structure.entity);
            }
        }
    }

    #[test]
    fn item_number_follows_released_product_convention() {
        let rp = seed_entities().into_iter().find(|e| e.structure.entity == "ReleasedProductsV2").unwrap();
        assert!(rp.structure.fields.iter().any(|f| f.name == "ItemNumber" && f.type_token == "ItemId"),
            "ReleasedProductsV2 must key on ItemNumber (ItemId), the Dynamics 365 replacement for SAP MATNR");
    }

    #[test]
    fn general_journal_account_entry_is_the_subledger_truth() {
        let gjae = seed_entities().into_iter().find(|e| e.structure.entity == "GeneralJournalAccountEntry");
        let gjae = gjae.expect("GeneralJournalAccountEntry must be present");
        assert!(gjae.structure.legacy_mapping.as_deref().unwrap_or("").contains("universal journal"),
            "GeneralJournalAccountEntry must be marked as the universal-journal source of truth");
    }

    #[test]
    fn legacy_tables_map_to_data_entities() {
        // BSEG → LedgerJournalTrans, ACDOCA → GeneralJournalAccountEntry, etc.
        for name in ["LedgerJournalTrans", "GeneralJournalAccountEntry", "ReleasedProductsV2"] {
            let e = seed_entities().into_iter().find(|e| e.structure.entity == name).unwrap();
            assert!(e.structure.legacy_mapping.is_some(),
                "entity '{name}' should carry a SAP→Dynamics 365 mapping note for migrating operators");
        }
    }
}
