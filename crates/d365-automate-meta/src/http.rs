//! Live Metadata API / X++ (AOT) client (feature `http`).
//!
//! Reads X++ / AOT objects over the Finance & Operations **Metadata API**
//! (`/metadata`) and data-entity rows over OData (`/data`), authenticated with
//! Microsoft Entra ID. A drop-in for `MockMetadataClient` behind the same
//! [`MetadataClient`] trait.
//!
//! Two operations are intentionally **not** served over this transport:
//!   - `cross_reference` — the build-time cross-reference (xRef) database is not
//!     exposed as a Metadata OData feed, so this returns an empty result.
//!   - `deploy` — deployment is performed via LCS deployable packages, not the
//!     Metadata API; this returns a clear `Forbidden` directing the operator to
//!     the package pipeline (the read-only gate is still honoured first).

use crate::client::{MetaCallContext, MetadataClient};
use crate::connection::{D365Auth, D365Connection};
use crate::error::{MetaError, MetaResult};
use crate::types::{
    CrossReferenceHit, CrossReferenceRequest, DataEntityView, DeployOutcome, DeployRequest,
    EntityRow, MetaSearchHit, MetaSearchRequest, ModelContents, ObjectSource, XppObjectKind,
    MAX_ENTITY_ROWS,
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::debug;

#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: Instant,
}

pub struct HttpMetadataClient {
    http: reqwest::Client,
    connection: D365Connection,
    authority_base: String,
    token_cache: Mutex<Option<CachedToken>>,
}

impl HttpMetadataClient {
    pub fn new(connection: D365Connection) -> MetaResult<Arc<Self>> {
        if matches!(connection.auth, D365Auth::Mock) {
            return Err(MetaError::AuthFailed(
                "HttpMetadataClient requires a non-mock connection".into(),
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("d365-automate/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| MetaError::Internal(format!("http client build: {e}")))?;
        Ok(Arc::new(Self {
            http,
            connection,
            authority_base: "https://login.microsoftonline.com".into(),
            token_cache: Mutex::new(None),
        }))
    }

    fn meta_url(&self, kind: XppObjectKind, name: &str) -> String {
        format!(
            "{}{}",
            self.connection.base_url.trim_end_matches('/'),
            kind.metadata_path(name)
        )
    }

    fn data_url(&self, entity: &str) -> String {
        format!(
            "{}/data/{}",
            self.connection.base_url.trim_end_matches('/'),
            entity
        )
    }

    /// Acquire a bearer token from the connection's auth.
    async fn token(&self) -> MetaResult<String> {
        match &self.connection.auth {
            D365Auth::Bearer { token } => Ok(token.clone()),
            D365Auth::ClientCredentials {
                tenant_id,
                client_id,
                client_secret,
            } => {
                self.client_credentials_token(tenant_id, client_id, client_secret)
                    .await
            }
            D365Auth::Certificate { .. } => Err(MetaError::AuthFailed(
                "certificate auth not yet implemented".into(),
            )),
            D365Auth::Mock => Err(MetaError::AuthFailed("mock connection has no token".into())),
        }
    }

    async fn client_credentials_token(
        &self,
        tenant_id: &str,
        client_id: &str,
        client_secret: &str,
    ) -> MetaResult<String> {
        if let Ok(guard) = self.token_cache.lock() {
            if let Some(t) = guard.as_ref() {
                if t.expires_at > Instant::now() {
                    return Ok(t.token.clone());
                }
            }
        }
        let scope = format!(
            "{}/.default",
            self.connection.base_url.trim_end_matches('/')
        );
        let endpoint = format!(
            "{}/{}/oauth2/v2.0/token",
            self.authority_base.trim_end_matches('/'),
            tenant_id
        );
        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("scope", scope.as_str()),
        ];
        let resp = self
            .http
            .post(&endpoint)
            .form(&form)
            .send()
            .await
            .map_err(map_reqwest)?;
        let status = resp.status();
        let body = resp.text().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(MetaError::AuthFailed(format!(
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
            .map_err(|e| MetaError::AuthFailed(format!("token parse: {e}")))?;
        let ttl = tok.expires_in.unwrap_or(3600).saturating_sub(60).max(1);
        if let Ok(mut guard) = self.token_cache.lock() {
            *guard = Some(CachedToken {
                token: tok.access_token.clone(),
                expires_at: Instant::now() + Duration::from_secs(ttl),
            });
        }
        Ok(tok.access_token)
    }

    async fn get_json(&self, url: &str) -> MetaResult<serde_json::Value> {
        let token = self.token().await?;
        debug!(url = %url, "GET metadata");
        let resp = self
            .http
            .get(url)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(map_reqwest)?;
        let status = resp.status();
        let body = resp.text().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(map_status(status.as_u16(), &body));
        }
        serde_json::from_str(&body).map_err(|e| MetaError::Internal(format!("metadata parse: {e}")))
    }

    async fn get_object(&self, kind: XppObjectKind, name: &str) -> MetaResult<ObjectSource> {
        let v = self.get_json(&self.meta_url(kind, name)).await?;
        Ok(object_from_json(kind, name, &v))
    }
}

#[async_trait]
impl MetadataClient for HttpMetadataClient {
    fn connection(&self) -> &D365Connection {
        &self.connection
    }

    async fn get_class(&self, name: &str) -> MetaResult<ObjectSource> {
        self.get_object(XppObjectKind::Class, name).await
    }
    async fn get_interface(&self, name: &str) -> MetaResult<ObjectSource> {
        self.get_object(XppObjectKind::Interface, name).await
    }
    async fn get_table(&self, name: &str) -> MetaResult<ObjectSource> {
        self.get_object(XppObjectKind::Table, name).await
    }
    async fn get_job(&self, name: &str) -> MetaResult<ObjectSource> {
        self.get_object(XppObjectKind::Job, name).await
    }
    async fn get_form(&self, name: &str) -> MetaResult<ObjectSource> {
        self.get_object(XppObjectKind::Form, name).await
    }

    async fn get_model_contents(&self, model: &str) -> MetaResult<ModelContents> {
        // ModelInfos is a flat feed; members are not enumerable in one call,
        // so we return the model header and leave members to `search`.
        let _ = self
            .get_json(&self.meta_url(XppObjectKind::Model, model))
            .await?;
        Ok(ModelContents {
            model: model.into(),
            description: None,
            members: Vec::new(),
        })
    }

    async fn get_data_entity(&self, name: &str) -> MetaResult<DataEntityView> {
        let v = self
            .get_json(&self.meta_url(XppObjectKind::DataEntity, name))
            .await?;
        let public = v
            .get("PublicCollectionName")
            .or_else(|| v.get("PublicEntityName"))
            .and_then(|x| x.as_str())
            .unwrap_or(name)
            .to_string();
        let source = v
            .get("SourceCode")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        Ok(DataEntityView {
            name: name.into(),
            public_entity_name: public,
            properties: v.clone(),
            source: source.clone(),
            line_count: source.lines().count(),
        })
    }

    async fn search(&self, request: MetaSearchRequest) -> MetaResult<Vec<MetaSearchHit>> {
        let collection = match request.kind.unwrap_or(XppObjectKind::DataEntity) {
            XppObjectKind::Class => "Classes",
            XppObjectKind::Table => "Tables",
            XppObjectKind::Interface => "Interfaces",
            _ => "DataEntities",
        };
        let escaped = request.query.replace('\'', "''");
        let url = format!(
            "{}/metadata/{}",
            self.connection.base_url.trim_end_matches('/'),
            collection
        );
        let token = self.token().await?;
        let resp = self
            .http
            .get(&url)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .query(&[
                ("$filter", format!("contains(Name,'{escaped}')")),
                ("$top", request.max_results.to_string()),
            ])
            .send()
            .await
            .map_err(map_reqwest)?;
        let status = resp.status();
        let body = resp.text().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(map_status(status.as_u16(), &body));
        }
        #[derive(serde::Deserialize)]
        struct Col {
            value: Vec<serde_json::Value>,
        }
        let col: Col = serde_json::from_str(&body)
            .map_err(|e| MetaError::Internal(format!("search parse: {e}")))?;
        let kind = request.kind.unwrap_or(XppObjectKind::DataEntity);
        Ok(col
            .value
            .iter()
            .map(|v| MetaSearchHit {
                name: v
                    .get("Name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                kind,
                description: v.get("Label").and_then(|x| x.as_str()).map(String::from),
                model: v.get("Model").and_then(|x| x.as_str()).map(String::from),
                score: 1.0,
            })
            .collect())
    }

    async fn cross_reference(
        &self,
        _request: CrossReferenceRequest,
    ) -> MetaResult<Vec<CrossReferenceHit>> {
        // The build-time xRef database is not exposed over the Metadata API.
        Ok(Vec::new())
    }

    async fn get_entity_contents(
        &self,
        entity: &str,
        max_rows: usize,
    ) -> MetaResult<Vec<EntityRow>> {
        if max_rows > MAX_ENTITY_ROWS {
            return Err(MetaError::EntityDataBlocked(format!(
                "requested {max_rows} rows exceeds the cap of {MAX_ENTITY_ROWS}; use d365.entity.read")));
        }
        let token = self.token().await?;
        let resp = self
            .http
            .get(self.data_url(entity))
            .bearer_auth(token)
            .header("Accept", "application/json")
            .query(&[("$top", max_rows.to_string())])
            .send()
            .await
            .map_err(map_reqwest)?;
        let status = resp.status();
        let body = resp.text().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(map_status(status.as_u16(), &body));
        }
        #[derive(serde::Deserialize)]
        struct Col {
            value: Vec<serde_json::Map<String, serde_json::Value>>,
        }
        let col: Col = serde_json::from_str(&body)
            .map_err(|e| MetaError::Internal(format!("entity parse: {e}")))?;
        Ok(col
            .value
            .into_iter()
            .map(|values| EntityRow { values })
            .collect())
    }

    async fn deploy(
        &self,
        request: DeployRequest,
        ctx: MetaCallContext,
    ) -> MetaResult<DeployOutcome> {
        if ctx.read_only {
            return Err(MetaError::PermissionDenied(format!(
                "deploy of {} '{}' requires write mode (--enable-writes)",
                request.kind.label(),
                request.name
            )));
        }
        Err(MetaError::Forbidden(
            "deployment is performed via LCS deployable packages, not the Metadata API — use the deployment pipeline".into()))
    }
}

/// Best-effort map of a Metadata API object payload to `ObjectSource`.
fn object_from_json(kind: XppObjectKind, name: &str, v: &serde_json::Value) -> ObjectSource {
    let source = v
        .get("SourceCode")
        .or_else(|| v.get("Declaration"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    ObjectSource {
        name: name.into(),
        kind,
        model: v.get("Model").and_then(|x| x.as_str()).map(String::from),
        description: v
            .get("Label")
            .or_else(|| v.get("Description"))
            .and_then(|x| x.as_str())
            .map(String::from),
        source: source.clone(),
        deployed: true,
        line_count: source.lines().count(),
    }
}

/// Load a connection by name from the standard search path (highest first):
/// `$D365_AUTOMATE_CONNECTION_DIR`, `./.d365-automate/connections`,
/// `~/.config/d365-automate/connections`.
pub fn load_connection(name: &str) -> MetaResult<D365Connection> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(d) = std::env::var("D365_AUTOMATE_CONNECTION_DIR") {
        dirs.push(PathBuf::from(d));
    }
    dirs.push(PathBuf::from("./.d365-automate/connections"));
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".config/d365-automate/connections"));
    }
    for dir in &dirs {
        let path = dir.join(format!("{name}.toml"));
        if path.exists() {
            return load_connection_file(&path, name);
        }
    }
    Err(MetaError::NotFound {
        kind: "Connection".into(),
        name: name.into(),
    })
}

fn load_connection_file(path: &Path, name: &str) -> MetaResult<D365Connection> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| MetaError::Internal(format!("read {}: {e}", path.display())))?;
    let mut conn: D365Connection = toml::from_str(&text)
        .map_err(|e| MetaError::Internal(format!("parse {}: {e}", path.display())))?;
    // The lookup key always wins over any on-disk label.
    conn.name = name.to_string();
    Ok(conn)
}

fn map_status(status: u16, body: &str) -> MetaError {
    match status {
        401 => MetaError::AuthFailed(truncate(body)),
        403 => MetaError::Forbidden(truncate(body)),
        404 => MetaError::NotFound {
            kind: "Object".into(),
            name: truncate(body),
        },
        408 => MetaError::Timeout { timeout_ms: 0 },
        429 | 503 => MetaError::RateLimited {
            retry_after_ms: 1000,
        },
        s if s >= 500 => MetaError::EnvironmentDown {
            environment: "live".into(),
            reason: format!("{s}: {}", truncate(body)),
        },
        s => MetaError::Internal(format!("unexpected status {s}: {}", truncate(body))),
    }
}

fn map_reqwest(e: reqwest::Error) -> MetaError {
    if e.is_timeout() {
        MetaError::Timeout { timeout_ms: 0 }
    } else if e.is_connect() {
        MetaError::EnvironmentDown {
            environment: "live".into(),
            reason: e.to_string(),
        }
    } else {
        MetaError::Internal(e.to_string())
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

    fn conn() -> D365Connection {
        D365Connection {
            name: "dev".into(),
            base_url: "https://gt-dev.operations.dynamics.com".into(),
            legal_entity: "USMF".into(),
            auth: D365Auth::ClientCredentials {
                tenant_id: "t".into(),
                client_id: "c".into(),
                client_secret: "s3cr3t".into(),
            },
        }
    }

    #[test]
    fn new_rejects_mock_connection() {
        assert!(HttpMetadataClient::new(D365Connection::mock("x")).is_err());
    }

    #[test]
    fn metadata_urls_are_well_formed() {
        let c = HttpMetadataClient::new(conn()).unwrap();
        assert_eq!(
            c.meta_url(XppObjectKind::Class, "GTFinJournalPoster"),
            "https://gt-dev.operations.dynamics.com/metadata/Classes('GTFinJournalPoster')"
        );
        assert_eq!(c.meta_url(XppObjectKind::DataEntity, "LedgerJournalLineEntity"),
            "https://gt-dev.operations.dynamics.com/metadata/DataEntities('LedgerJournalLineEntity')");
        assert_eq!(
            c.data_url("CustomersV3"),
            "https://gt-dev.operations.dynamics.com/data/CustomersV3"
        );
    }

    #[test]
    fn object_from_json_maps_fields() {
        let v = serde_json::json!({ "SourceCode": "class X\n{\n}\n", "Model": "GTFin", "Label": "Helper" });
        let o = object_from_json(XppObjectKind::Class, "X", &v);
        assert_eq!(o.model.as_deref(), Some("GTFin"));
        assert_eq!(o.description.as_deref(), Some("Helper"));
        assert_eq!(o.line_count, 3);
        assert!(o.deployed);
    }

    #[test]
    fn status_mapping() {
        assert!(matches!(map_status(401, ""), MetaError::AuthFailed(_)));
        assert!(matches!(map_status(403, ""), MetaError::Forbidden(_)));
        assert!(matches!(map_status(503, ""), MetaError::RateLimited { .. }));
        assert!(map_status(503, "").is_transient());
    }

    #[test]
    fn connection_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("d365meta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("acme.toml");
        std::fs::write(
            &path,
            r#"
base_url = "https://acme.operations.dynamics.com"
legal_entity = "USMF"
[auth]
type = "bearer"
token = "abc"
"#,
        )
        .unwrap();
        let conn = load_connection_file(&path, "acme").unwrap();
        assert_eq!(conn.name, "acme");
        assert_eq!(conn.base_url, "https://acme.operations.dynamics.com");
        assert!(matches!(conn.auth, D365Auth::Bearer { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
