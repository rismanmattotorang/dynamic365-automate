//! Request and response types shared by every Metadata backend.

use serde::{Deserialize, Serialize};

pub const MAX_ENTITY_ROWS: usize = 1000;

/// X++ / Application Object Tree (AOT) object kinds. The Dynamics 365 analog
/// of SAP's `AbapObjectKind`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum XppObjectKind {
    Class,
    Interface,
    Table,
    DataEntity,
    View,
    Form,
    Job,
    Query,
    EnumType,
    ExtendedDataType,
    Macro,
    Model,
    CustomService,
    MenuItem,
}

impl XppObjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Class => "Class",
            Self::Interface => "Interface",
            Self::Table => "Table",
            Self::DataEntity => "Data Entity",
            Self::View => "View",
            Self::Form => "Form",
            Self::Job => "Job",
            Self::Query => "Query",
            Self::EnumType => "Enum",
            Self::ExtendedDataType => "Extended Data Type",
            Self::Macro => "Macro",
            Self::Model => "Model",
            Self::CustomService => "Custom Service",
            Self::MenuItem => "Menu Item",
        }
    }

    /// F&O Metadata OData path fragment, e.g. `/metadata/Classes('GTFinPoster')`.
    pub fn metadata_path(self, name: &str) -> String {
        match self {
            Self::Class => format!("/metadata/Classes('{name}')"),
            Self::Interface => format!("/metadata/Interfaces('{name}')"),
            Self::Table => format!("/metadata/Tables('{name}')"),
            Self::DataEntity => format!("/metadata/DataEntities('{name}')"),
            Self::View => format!("/metadata/Views('{name}')"),
            Self::Form => format!("/metadata/Forms('{name}')"),
            Self::Job => format!("/metadata/Jobs('{name}')"),
            Self::Query => format!("/metadata/Queries('{name}')"),
            Self::EnumType => format!("/metadata/Enums('{name}')"),
            Self::ExtendedDataType => format!("/metadata/ExtendedDataTypes('{name}')"),
            Self::Macro => format!("/metadata/Macros('{name}')"),
            Self::Model => "/metadata/ModelInfos".to_string(),
            Self::CustomService => format!("/metadata/CustomServices('{name}')"),
            Self::MenuItem => format!("/metadata/MenuItems('{name}')"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSource {
    pub name: String,
    pub kind: XppObjectKind,
    /// Model the object belongs to (e.g. `GTFin`).
    pub model: Option<String>,
    /// Short description / label from the object header.
    pub description: Option<String>,
    pub source: String,
    /// Whether the object is currently deployed (vs. checked-out / undeployed).
    pub deployed: bool,
    /// Lines counted from the source.
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEntityView {
    pub name: String,
    pub source: String,
    /// Public entity name exposed over OData, e.g. `LedgerJournalLineEntity`.
    pub public_entity_name: String,
    /// Entity properties distilled into a structured map for quick access.
    pub properties: serde_json::Value,
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMember {
    pub name: String,
    pub kind: XppObjectKind,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelContents {
    pub model: String,
    pub description: Option<String>,
    pub members: Vec<ModelMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSearchRequest {
    pub query: String,
    #[serde(default)]
    pub kind: Option<XppObjectKind>,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    25
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSearchHit {
    pub name: String,
    pub kind: XppObjectKind,
    pub description: Option<String>,
    pub model: Option<String>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossReferenceRequest {
    pub name: String,
    pub kind: XppObjectKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossReferenceHit {
    pub object: String,
    pub kind: XppObjectKind,
    /// Where in the object the reference appears (e.g. method, line).
    pub location: String,
    /// e.g. `read`, `write`, `call`, `extends`, `implements`.
    pub usage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRow {
    pub values: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    pub name: String,
    pub kind: XppObjectKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployOutcome {
    pub name: String,
    pub kind: XppObjectKind,
    pub deployed: bool,
    pub messages: Vec<String>,
}
