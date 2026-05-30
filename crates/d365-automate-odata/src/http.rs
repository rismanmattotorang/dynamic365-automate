//! Live Finance & Operations OData v4 / Custom Service client (feature `http`).
//!
//! Talks to a real Dynamics 365 F&O environment over HTTPS, authenticated with
//! **Microsoft Entra ID** (OAuth 2.0 client-credentials). It is a drop-in for
//! `MockD365Client` behind the same [`D365Client`] trait, so the MCP server
//! swaps it in with a one-line construction change.
//!
//! Design (mirrors the SAP-Automate live tier): **metadata, search, and entity
//! structure are served from the curated catalogue** (`crate::client`) so the
//! read-only safety annotations + security stay stable, while **entity reads and
//! service calls hit the live environment**. Data ops are live; the safety
//! catalogue is curated.

use crate::client::{
    seed_entity_structures, seed_operations, BulkMetadata, D365Client, EntityRow, EntityStructure,
    EnvironmentInfo, ReadEntityRequest, ServiceCallRequest, ServiceOperationMeta,
    ServiceSearchResult, ServiceSummary, COMPANY_FIELD, MAX_ROWS_HARD_CAP,
};
use crate::credentials::Credentials;
use crate::error::{D365Error, D365Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::debug;

/// Connection config for a live environment.
#[derive(Clone)]
pub struct HttpD365Config {
    /// Environment resource, e.g. `https://gt-prod.operations.dynamics.com`.
    pub resource: String,
    /// Entra directory (tenant) id.
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
    /// Default legal entity (DataAreaId).
    pub legal_entity: String,
    /// Entra authority base (override in tests). Defaults to the public cloud.
    pub authority_base: String,
    pub timeout: Duration,
}

impl HttpD365Config {
    pub fn from_credentials(c: &Credentials) -> Self {
        Self {
            resource: c.resource.clone(),
            tenant_id: c.tenant_id.clone(),
            client_id: c.client_id.clone(),
            client_secret: c.client_secret.clone(),
            legal_entity: c.legal_entity.clone(),
            authority_base: "https://login.microsoftonline.com".into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// OAuth2 v2.0 token endpoint.
    pub fn token_endpoint(&self) -> String {
        format!(
            "{}/{}/oauth2/v2.0/token",
            self.authority_base.trim_end_matches('/'),
            self.tenant_id
        )
    }

    /// `.default` scope for the environment resource.
    pub fn scope(&self) -> String {
        format!("{}/.default", self.resource.trim_end_matches('/'))
    }

    fn validate(&self) -> D365Result<()> {
        for (name, v) in [
            ("resource", &self.resource),
            ("tenant_id", &self.tenant_id),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ] {
            if v.trim().is_empty() {
                return Err(D365Error::AuthFailed(format!("{name} must not be empty")));
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for HttpD365Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpD365Config")
            .field("resource", &self.resource)
            .field("tenant_id", &self.tenant_id)
            .field("client_id", &self.client_id)
            .field("client_secret", &"***")
            .field("legal_entity", &self.legal_entity)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: Instant,
}

pub struct HttpD365Client {
    http: reqwest::Client,
    config: HttpD365Config,
    token_cache: Mutex<Option<CachedToken>>,
    operations: HashMap<String, ServiceOperationMeta>,
    entities: HashMap<String, EntityStructure>,
}

impl HttpD365Client {
    pub fn new(config: HttpD365Config) -> D365Result<Arc<Self>> {
        config.validate()?;
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent(concat!("d365-automate/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| D365Error::Internal(format!("http client build: {e}")))?;
        let operations = seed_operations()
            .into_iter()
            .map(|o| (o.operation.clone(), o))
            .collect();
        let entities = seed_entity_structures()
            .into_iter()
            .map(|s| (s.entity.clone(), s))
            .collect();
        Ok(Arc::new(Self {
            http,
            config,
            token_cache: Mutex::new(None),
            operations,
            entities,
        }))
    }

    /// Build from the process environment (`D365_*`). Returns `None` when no
    /// `D365_RESOURCE` is set, so CI without secrets skips cleanly.
    pub fn from_env() -> Option<D365Result<Arc<Self>>> {
        let resource = std::env::var("D365_RESOURCE")
            .ok()
            .filter(|s| !s.is_empty())?;
        let cfg = HttpD365Config {
            resource,
            tenant_id: std::env::var("D365_TENANT_ID").unwrap_or_default(),
            client_id: std::env::var("D365_CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("D365_CLIENT_SECRET").unwrap_or_default(),
            legal_entity: std::env::var("D365_LEGAL_ENTITY").unwrap_or_else(|_| "USMF".into()),
            authority_base: "https://login.microsoftonline.com".into(),
            timeout: Duration::from_secs(30),
        };
        Some(Self::new(cfg))
    }

    pub fn config(&self) -> &HttpD365Config {
        &self.config
    }

    fn data_url(&self, path: &str) -> String {
        format!(
            "{}/data/{}",
            self.config.resource.trim_end_matches('/'),
            path
        )
    }

    /// Acquire (or reuse) an Entra ID access token via the client-credentials grant.
    async fn token(&self) -> D365Result<String> {
        if let Ok(guard) = self.token_cache.lock() {
            if let Some(t) = guard.as_ref() {
                if t.expires_at > Instant::now() {
                    return Ok(t.token.clone());
                }
            }
        }
        let scope = self.config.scope();
        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("scope", scope.as_str()),
        ];
        let resp = self
            .http
            .post(self.config.token_endpoint())
            .form(&form)
            .send()
            .await
            .map_err(|e| map_reqwest(&e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| map_reqwest(&e))?;
        if !status.is_success() {
            return Err(D365Error::AuthFailed(format!(
                "token endpoint {}: {}",
                status.as_u16(),
                truncate(&body)
            )));
        }
        #[derive(serde::Deserialize)]
        struct TokenResp {
            access_token: String,
            #[serde(default)]
            expires_in: Option<u64>,
        }
        let tok: TokenResp = serde_json::from_str(&body)
            .map_err(|e| D365Error::AuthFailed(format!("token response parse: {e}")))?;
        let ttl = tok.expires_in.unwrap_or(3600).saturating_sub(60).max(1);
        if let Ok(mut guard) = self.token_cache.lock() {
            *guard = Some(CachedToken {
                token: tok.access_token.clone(),
                expires_at: Instant::now() + Duration::from_secs(ttl),
            });
        }
        Ok(tok.access_token)
    }
}

#[async_trait]
impl D365Client for HttpD365Client {
    async fn environment_info(&self) -> D365Result<EnvironmentInfo> {
        // Synthesised from config + identity; no mandatory network round-trip.
        let host = self
            .config
            .resource
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('.')
            .next()
            .unwrap_or("d365")
            .to_string();
        Ok(EnvironmentInfo {
            environment: host,
            legal_entity: self.config.legal_entity.clone(),
            version: "Dynamics 365 Finance & Operations (live)".into(),
            environment_role: std::env::var("D365_ENVIRONMENT_ROLE")
                .unwrap_or_else(|_| "PROD".into()),
            base_url: self.config.resource.clone(),
            platform: "Finance & Operations".into(),
            identity: serde_json::json!({
                "resource": self.config.resource,
                "tenant_id": self.config.tenant_id,
                "client_id": self.config.client_id,
                "client_secret": "***",
                "legal_entity": self.config.legal_entity,
                "source": "HttpD365Client",
            }),
        })
    }

    async fn search_service(&self, query: &str, limit: usize) -> D365Result<ServiceSearchResult> {
        // Served from the curated catalogue.
        let q = query.to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        let mut hits: Vec<ServiceSummary> = self
            .operations
            .values()
            .filter_map(|f| {
                let hay = format!(
                    "{} {} {}",
                    f.operation.to_lowercase(),
                    f.description.to_lowercase(),
                    f.service_group.to_lowercase()
                );
                let score: usize = terms.iter().map(|t| hay.matches(t).count()).sum();
                (score > 0).then(|| ServiceSummary {
                    operation: f.operation.clone(),
                    description: f.description.clone(),
                    service_group: f.service_group.clone(),
                    read_only: f.read_only,
                    score: score as f32,
                })
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(limit.max(1));
        Ok(ServiceSearchResult {
            query: query.into(),
            hits,
        })
    }

    async fn service_metadata(
        &self,
        operation: &str,
        _language: &str,
    ) -> D365Result<ServiceOperationMeta> {
        self.operations
            .get(operation)
            .cloned()
            .ok_or_else(|| D365Error::NotFound(operation.into()))
    }

    async fn bulk_service_metadata(
        &self,
        operations: &[String],
        language: &str,
    ) -> D365Result<BulkMetadata> {
        let mut out = Vec::new();
        let mut missing = Vec::new();
        for op in operations {
            match self.operations.get(op) {
                Some(m) => out.push(m.clone()),
                None => missing.push(op.clone()),
            }
        }
        Ok(BulkMetadata {
            language: language.into(),
            operations: out,
            missing,
        })
    }

    async fn call_service(
        &self,
        request: ServiceCallRequest,
        read_only_mode: bool,
    ) -> D365Result<serde_json::Value> {
        // The curated catalogue drives the read-only safety gate.
        if let Some(meta) = self.operations.get(&request.operation) {
            if read_only_mode && !meta.read_only {
                return Err(D365Error::PermissionDenied(format!(
                    "operation '{}' modifies state; not callable in read-only mode",
                    request.operation
                )));
            }
        }
        // A single unbound OData action POST is atomic server-side; multi-op
        // change-set batching (POST /data/$batch) is a follow-up.
        let url = self.data_url(&request.operation);
        let token = self.token().await?;
        debug!(url = %url, "POST OData action");
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .json(&request.parameters)
            .send()
            .await
            .map_err(|e| map_reqwest(&e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| map_reqwest(&e))?;
        if !status.is_success() {
            return Err(map_status(status.as_u16(), &body));
        }
        Ok(serde_json::from_str(&body).unwrap_or(serde_json::Value::String(body)))
    }

    async fn read_entity(&self, request: ReadEntityRequest) -> D365Result<Vec<EntityRow>> {
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
        let company_scoped = self
            .entities
            .get(&request.entity)
            .map(|e| e.company_scoped)
            .unwrap_or(false);
        let query = build_odata_query(&request, &self.config.legal_entity, company_scoped);
        let url = self.data_url(&request.entity);
        let token = self.token().await?;
        debug!(url = %url, "GET OData entity");
        let resp = self
            .http
            .get(&url)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .query(&query)
            .send()
            .await
            .map_err(|e| map_reqwest(&e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| map_reqwest(&e))?;
        if !status.is_success() {
            return Err(map_status(status.as_u16(), &body));
        }
        #[derive(serde::Deserialize)]
        struct Collection {
            value: Vec<serde_json::Map<String, serde_json::Value>>,
        }
        let col: Collection = serde_json::from_str(&body)
            .map_err(|e| D365Error::SchemaViolation(format!("OData collection parse: {e}")))?;
        Ok(col
            .value
            .into_iter()
            .map(|values| EntityRow { values })
            .collect())
    }

    async fn entity_structure(&self, entity: &str) -> D365Result<EntityStructure> {
        self.entities
            .get(entity)
            .cloned()
            .ok_or_else(|| D365Error::NotFound(entity.into()))
    }
}

/// Build the OData v4 query parameters: `$select`, `$filter`, `$top`, injecting
/// a `dataAreaId` filter for company-scoped entities when the caller omits one.
fn build_odata_query(
    req: &ReadEntityRequest,
    legal_entity: &str,
    company_scoped: bool,
) -> Vec<(String, String)> {
    let mut q: Vec<(String, String)> = Vec::new();
    if !req.fields.is_empty() {
        q.push(("$select".into(), req.fields.join(",")));
    }
    let mut clauses: Vec<String> = req.filters.iter().map(|f| translate_filter(f)).collect();
    if company_scoped
        && !req
            .filters
            .iter()
            .any(|f| f.to_lowercase().contains(&COMPANY_FIELD.to_lowercase()))
    {
        clauses.push(format!(
            "{} eq '{}'",
            COMPANY_FIELD,
            legal_entity.replace('\'', "''")
        ));
    }
    if !clauses.is_empty() {
        q.push(("$filter".into(), clauses.join(" and ")));
    }
    q.push(("$top".into(), req.max_rows.to_string()));
    q
}

/// Translate the crate's filter mini-syntax to valid OData v4.
/// `Field like '%x%'` → `contains(Field,'x')`; `Field eq 'v'` passes through.
fn translate_filter(clause: &str) -> String {
    let upper = clause.to_uppercase();
    if let Some(idx) = upper.find(" LIKE ") {
        let field = clause[..idx].trim();
        let raw = clause[idx + 6..].trim().trim_matches('\'');
        let needle = raw.trim_matches('%').replace('\'', "''");
        if raw.starts_with('%') && raw.ends_with('%') {
            return format!("contains({field},'{needle}')");
        } else if raw.ends_with('%') {
            return format!("startswith({field},'{needle}')");
        } else if raw.starts_with('%') {
            return format!("endswith({field},'{needle}')");
        }
        return format!("{field} eq '{needle}'");
    }
    clause.to_string()
}

fn map_status(status: u16, body: &str) -> D365Error {
    match status {
        401 => D365Error::AuthFailed(truncate(body)),
        403 => D365Error::PermissionDenied(truncate(body)),
        404 => D365Error::NotFound(truncate(body)),
        408 => D365Error::Timeout { timeout_ms: 0 },
        429 | 503 => D365Error::EnvironmentDown {
            environment: "live".into(),
            reason: format!("{status}: {}", truncate(body)),
        },
        400 | 422 => D365Error::SchemaViolation(truncate(body)),
        s if s >= 500 => D365Error::EnvironmentDown {
            environment: "live".into(),
            reason: format!("{s}: {}", truncate(body)),
        },
        s => D365Error::Internal(format!("unexpected status {s}: {}", truncate(body))),
    }
}

fn map_reqwest(e: &reqwest::Error) -> D365Error {
    if e.is_timeout() {
        D365Error::Timeout { timeout_ms: 0 }
    } else if e.is_connect() {
        D365Error::EnvironmentDown {
            environment: "live".into(),
            reason: e.to_string(),
        }
    } else {
        D365Error::Internal(e.to_string())
    }
}

fn truncate(s: &str) -> String {
    let limit = 300;
    if s.len() <= limit {
        s.to_string()
    } else {
        format!("{}…", &s[..limit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> HttpD365Config {
        HttpD365Config {
            resource: "https://gt-test.operations.dynamics.com".into(),
            tenant_id: "tenant-123".into(),
            client_id: "client-abc".into(),
            client_secret: "s3cr3t-value".into(),
            legal_entity: "USMF".into(),
            authority_base: "https://login.microsoftonline.com".into(),
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn token_endpoint_and_scope() {
        let c = cfg();
        assert_eq!(
            c.token_endpoint(),
            "https://login.microsoftonline.com/tenant-123/oauth2/v2.0/token"
        );
        assert_eq!(
            c.scope(),
            "https://gt-test.operations.dynamics.com/.default"
        );
    }

    #[test]
    fn debug_redacts_secret() {
        let dbg = format!("{:?}", cfg());
        assert!(!dbg.contains("s3cr3t-value"), "secret value leaked: {dbg}");
        assert!(dbg.contains("***"));
    }

    #[test]
    fn validate_rejects_empty() {
        let mut c = cfg();
        c.client_secret = String::new();
        assert!(c.validate().is_err());
    }

    #[test]
    fn query_injects_company_scope_and_select_top() {
        let req = ReadEntityRequest {
            entity: "FiscalCalendarPeriod".into(),
            fields: vec!["FiscalCalendarPeriodName".into(), "Status".into()],
            filters: vec![],
            max_rows: 50,
        };
        let q = build_odata_query(&req, "USMF", true);
        assert!(q.contains(&("$select".into(), "FiscalCalendarPeriodName,Status".into())));
        assert!(q.contains(&("$filter".into(), "dataAreaId eq 'USMF'".into())));
        assert!(q.contains(&("$top".into(), "50".into())));
    }

    #[test]
    fn query_respects_explicit_company_filter() {
        let req = ReadEntityRequest {
            entity: "FiscalCalendarPeriod".into(),
            fields: vec![],
            filters: vec!["dataAreaId eq 'GBSI'".into()],
            max_rows: 100,
        };
        let q = build_odata_query(&req, "USMF", true);
        let filter = q.iter().find(|(k, _)| k == "$filter").unwrap();
        assert_eq!(filter.1, "dataAreaId eq 'GBSI'");
        assert!(!filter.1.contains("USMF"));
    }

    #[test]
    fn translate_like_to_contains() {
        assert_eq!(
            translate_filter("OrganizationName like '%Forest%'"),
            "contains(OrganizationName,'Forest')"
        );
        assert_eq!(
            translate_filter("Name like 'Acme%'"),
            "startswith(Name,'Acme')"
        );
        assert_eq!(
            translate_filter("CustomerAccount eq 'US-001'"),
            "CustomerAccount eq 'US-001'"
        );
    }

    #[test]
    fn status_mapping() {
        assert!(matches!(map_status(401, ""), D365Error::AuthFailed(_)));
        assert!(matches!(
            map_status(403, ""),
            D365Error::PermissionDenied(_)
        ));
        assert!(matches!(map_status(404, ""), D365Error::NotFound(_)));
        assert!(matches!(
            map_status(503, ""),
            D365Error::EnvironmentDown { .. }
        ));
        assert!(map_status(503, "").is_transient());
        assert!(!map_status(404, "").is_transient());
    }
}
