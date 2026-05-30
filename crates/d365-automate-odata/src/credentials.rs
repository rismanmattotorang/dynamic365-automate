//! Layered credential provider for Microsoft Entra ID (Azure AD).
//!
//! Dynamics 365 F&O authenticates via an **Entra ID app registration** using
//! the OAuth 2.0 client-credentials grant: a `client_id` + `client_secret`
//! (or certificate) for a `tenant_id` is exchanged for a bearer token scoped
//! to the environment resource (`https://<env>.operations.dynamics.com`).
//!
//! Callers compose any number of providers in any order; the first that
//! yields credentials wins. Ships `EnvCredentialProvider` (env vars) and
//! `StaticCredentialProvider` (literal values, for tests). The token exchange
//! itself lands with the live HTTP client in Phase 3b.

use crate::error::{D365Error, D365Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Entra ID client-credentials for one Dynamics 365 environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    /// e.g. `https://gt-prod.operations.dynamics.com`
    pub resource: String,
    /// Entra ID tenant (directory) id.
    pub tenant_id: String,
    /// App registration (client) id.
    pub client_id: String,
    /// App registration secret.  Stored only for the process lifetime; never logged.
    #[serde(skip_serializing)]
    pub client_secret: String,
    /// Default legal entity (DataAreaId), e.g. `USMF`.
    pub legal_entity: String,
    /// Where this credential came from (for audit logs).
    pub source: CredentialSource,
}

impl Credentials {
    /// Authority URL for the OAuth2 token endpoint.
    pub fn token_endpoint(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.tenant_id
        )
    }

    /// Redacted summary safe for logs and the `d365-env://info` resource.
    pub fn redacted(&self) -> serde_json::Value {
        serde_json::json!({
            "resource": self.resource,
            "tenant_id": self.tenant_id,
            "client_id": self.client_id,
            "legal_entity": self.legal_entity,
            "source": format!("{:?}", self.source),
            "client_secret": "***",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CredentialSource {
    Env,
    Keyring,
    EncryptedFile,
    DotEnv,
    Static,
    None,
}

#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// Returns `Ok(None)` if this provider has no credentials configured
    /// (so the caller can move to the next in the chain).
    async fn fetch(&self) -> D365Result<Option<Credentials>>;
}

// ---------------------------------------------------------------------------
// Environment provider
// ---------------------------------------------------------------------------

pub struct EnvCredentialProvider;

impl EnvCredentialProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnvCredentialProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialProvider for EnvCredentialProvider {
    async fn fetch(&self) -> D365Result<Option<Credentials>> {
        let needed = [
            "D365_RESOURCE",
            "D365_TENANT_ID",
            "D365_CLIENT_ID",
            "D365_CLIENT_SECRET",
        ];
        let present: Vec<_> = needed
            .iter()
            .filter(|k| std::env::var(*k).is_ok())
            .collect();
        if present.is_empty() {
            return Ok(None);
        }
        if present.len() < needed.len() {
            return Err(D365Error::AuthFailed(format!(
                "partial Dynamics 365 env vars: missing {:?}",
                needed
                    .iter()
                    .filter(|k| std::env::var(*k).is_err())
                    .collect::<Vec<_>>(),
            )));
        }
        Ok(Some(Credentials {
            resource: std::env::var("D365_RESOURCE").unwrap(),
            tenant_id: std::env::var("D365_TENANT_ID").unwrap(),
            client_id: std::env::var("D365_CLIENT_ID").unwrap(),
            client_secret: std::env::var("D365_CLIENT_SECRET").unwrap(),
            legal_entity: std::env::var("D365_LEGAL_ENTITY").unwrap_or_else(|_| "USMF".to_string()),
            source: CredentialSource::Env,
        }))
    }
}

// ---------------------------------------------------------------------------
// Static provider (tests, demos)
// ---------------------------------------------------------------------------

pub struct StaticCredentialProvider {
    creds: Credentials,
}

impl StaticCredentialProvider {
    pub fn new(creds: Credentials) -> Self {
        Self { creds }
    }
}

#[async_trait]
impl CredentialProvider for StaticCredentialProvider {
    async fn fetch(&self) -> D365Result<Option<Credentials>> {
        Ok(Some(self.creds.clone()))
    }
}

// ---------------------------------------------------------------------------
// Layered chain
// ---------------------------------------------------------------------------

/// Tries each underlying provider in order; the first that returns
/// `Some(creds)` wins.  Returns `Ok(None)` only if every provider was empty.
pub struct LayeredCredentialProvider {
    providers: Vec<Arc<dyn CredentialProvider>>,
}

impl LayeredCredentialProvider {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, p: Arc<dyn CredentialProvider>) -> Self {
        self.providers.push(p);
        self
    }
}

impl Default for LayeredCredentialProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialProvider for LayeredCredentialProvider {
    async fn fetch(&self) -> D365Result<Option<Credentials>> {
        for p in &self.providers {
            match p.fetch().await {
                Ok(Some(c)) => return Ok(Some(c)),
                Ok(None) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(source: CredentialSource, resource: &str) -> Credentials {
        Credentials {
            resource: resource.into(),
            tenant_id: "11111111-1111-1111-1111-111111111111".into(),
            client_id: "app-client-id".into(),
            client_secret: "x".into(),
            legal_entity: "USMF".into(),
            source,
        }
    }

    #[tokio::test]
    async fn static_provider_returns_credentials() {
        let p = StaticCredentialProvider::new(creds(
            CredentialSource::Static,
            "https://gt.operations.dynamics.com",
        ));
        let c = p.fetch().await.unwrap().unwrap();
        assert_eq!(c.legal_entity, "USMF");
        let r = c.redacted();
        assert_eq!(r["client_secret"], "***");
        assert!(c.token_endpoint().contains("login.microsoftonline.com"));
    }

    #[tokio::test]
    async fn layered_falls_through() {
        let layered = LayeredCredentialProvider::new()
            .add(Arc::new(EnvCredentialProvider::new())) // unset env => None
            .add(Arc::new(StaticCredentialProvider::new(creds(
                CredentialSource::Static,
                "https://fallback.operations.dynamics.com",
            ))));
        let c = layered.fetch().await.unwrap().unwrap();
        assert_eq!(c.resource, "https://fallback.operations.dynamics.com");
    }
}
