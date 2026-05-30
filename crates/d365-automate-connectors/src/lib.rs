//! Dynamics 365 connectors.
//!
//! Ships the trait surface; concrete Metadata API / Power Automate / Dataverse
//! solution HTTP clients land in later phases.

use std::future::Future;
use std::pin::Pin;

/// Reads X++ / AOT objects.
pub trait XppConnector: Send + Sync {
    fn read_object<'a>(
        &'a self,
        model: &'a str,
        name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ConnectorError>> + Send + 'a>>;
}

/// Reads Power Automate flow / business-process definitions.
pub trait FlowConnector: Send + Sync {
    fn read_definition<'a>(
        &'a self,
        process_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ConnectorError>> + Send + 'a>>;
}

/// Reads Dataverse solution / module fact sheets.
pub trait SolutionConnector: Send + Sync {
    fn read_fact_sheet<'a>(
        &'a self,
        id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ConnectorError>> + Send + 'a>>;
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("upstream error: {0}")]
    Upstream(String),
    #[error("auth error: {0}")]
    Auth(String),
}
