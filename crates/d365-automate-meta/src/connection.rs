//! Connection model.
//!
//! A connection is a named bundle of (base URL, legal entity, auth method).
//! Connections live as TOML files under
//! `~/.config/d365-automate/connections/<name>.toml`. This module ships the
//! type + a synchronous in-memory builder; loading from disk lives behind the
//! `http` feature.

use serde::{Deserialize, Serialize};

/// One connection = one Dynamics 365 environment endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct D365Connection {
    /// Logical name (the lookup key the file is selected under always wins).
    #[serde(default)]
    pub name: String,
    /// e.g. `https://gt-prod.operations.dynamics.com`
    pub base_url: String,
    /// Default legal entity / DataAreaId, e.g. `USMF`.
    pub legal_entity: String,
    pub auth: D365Auth,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum D365Auth {
    /// Microsoft Entra ID OAuth2 client-credentials (the standard F&O path).
    ClientCredentials {
        tenant_id: String,
        client_id: String,
        client_secret: String,
    },
    /// Pre-acquired bearer token.
    Bearer { token: String },
    /// Entra ID certificate credential (cert + key PEM paths).
    Certificate {
        tenant_id: String,
        client_id: String,
        cert_path: String,
        key_path: String,
    },
    /// Mock connection — no network at all.
    Mock,
}

impl D365Connection {
    pub fn mock(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_url: "https://gt-dev.operations.dynamics.com".into(),
            legal_entity: "USMF".into(),
            auth: D365Auth::Mock,
        }
    }

    /// Redacted form for logs / `d365-meta://info` resource.
    pub fn redacted(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "base_url": self.base_url,
            "legal_entity": self.legal_entity,
            "auth_type": auth_type_label(&self.auth),
        })
    }
}

fn auth_type_label(auth: &D365Auth) -> &'static str {
    match auth {
        D365Auth::ClientCredentials { .. } => "client_credentials",
        D365Auth::Bearer { .. } => "bearer",
        D365Auth::Certificate { .. } => "certificate",
        D365Auth::Mock => "mock",
    }
}

impl std::fmt::Debug for D365Auth {
    /// Never print secrets.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "D365Auth::{}", auth_type_label(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_hides_secrets() {
        let c = D365Connection {
            name: "prod".into(),
            base_url: "https://gt-prod.operations.dynamics.com".into(),
            legal_entity: "USMF".into(),
            auth: D365Auth::ClientCredentials {
                tenant_id: "t".into(),
                client_id: "c".into(),
                client_secret: "super-secret".into(),
            },
        };
        let r = c.redacted();
        assert_eq!(r["auth_type"], "client_credentials");
        assert!(!r.to_string().contains("super-secret"));
        // Debug must not leak the secret either.
        assert!(!format!("{:?}", c.auth).contains("super-secret"));
    }

    #[test]
    fn mock_connection_defaults() {
        let c = D365Connection::mock("dev");
        assert!(matches!(c.auth, D365Auth::Mock));
        assert_eq!(c.legal_entity, "USMF");
    }
}
