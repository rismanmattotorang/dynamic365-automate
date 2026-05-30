//! Typed entities and edges.
//!
//! The cross-domain Dynamics 365 knowledge graph has six entity families and a
//! small, fixed set of edge kinds.  Both can be extended as new Dynamics 365
//! domains (Dataverse, Power Platform, etc.) come online without breaking
//! consumers — community summaries depend on this stability.

use serde::{Deserialize, Serialize};

pub type NodeId = String;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// X++ object: class, interface, job, form, table, or method.
    XppObject,
    /// Dynamics 365 data entity / backing table.
    DataEntity,
    /// Data-entity field / column.
    Field,
    /// OData action or Custom Service operation.
    Service,
    /// Power Automate flow / business process.
    Flow,
    /// Dataverse solution / Dynamics 365 module.
    Solution,
    /// Microsoft Learn page or section.
    LearnPage,
    /// Business concept (e.g. "period close", "product receipt").  These
    /// are the nodes that let GraphRAG community summaries cross domains.
    Concept,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: NodeId,
    pub kind: EntityKind,
    pub label: String,
    /// Short description, used for community summaries.
    #[serde(default)]
    pub description: Option<String>,
    /// Native URI for citation (d365-learn://, xpp-obj://, d365-service://, etc.).
    #[serde(default)]
    pub uri: Option<String>,
    /// Arbitrary string-valued tags ("module:GeneralLedger", "model:GTFin", ...).
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// X++ object A calls / invokes B.
    Calls,
    /// X++ class implements interface.
    Implements,
    /// Object uses / references a helper class or shared definition.
    Uses,
    /// Object reads from a data entity.
    ReadsEntity,
    /// Object writes to a data entity.
    WritesEntity,
    /// One entity references / mentions another in its documentation.
    References,
    /// Entity is contained in a parent (class in model, field in entity).
    ContainedIn,
    /// Entity depends on another (flow step depends on service, app depends on entity).
    DependsOn,
    /// Concept describes / categorises an entity.
    Describes,
    /// Free-form relationship — last-resort kind that should still be
    /// rare enough that GraphRAG community summaries remain meaningful.
    Related,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    /// Optional weight; defaults to 1.0.  PPR uses it; community detection
    /// treats it as an edge multiplicity.
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_weight() -> f32 { 1.0 }
