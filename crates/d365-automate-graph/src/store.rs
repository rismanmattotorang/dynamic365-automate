//! In-memory graph store seeded with realistic cross-domain Dynamics 365 fixtures.
//!
//! The same fixtures the OData + Metadata + KB mocks expose, but stitched into
//! one graph so multi-hop traversal demos are meaningful offline.
//!
//! Example dependency chain (encoded below):
//!
//!   `GTFinPostable` (interface)
//!       ←implements← `GTFinJournalPoster` (class)
//!           ←calls← `GTFinPostJournal` (job)
//!               ↓uses
//!           `GTFinConstants`, `GTFinValidation`
//!       ↓calls
//!     `LedgerGeneralJournalEntry` (OData service)
//!       ↓reads_entity
//!     `CompaniesV2`, `FiscalCalendarPeriod`
//!       ↓describes
//!     `Concept: period_close`
//!       ←contained_in← `Flow: Order-to-Cash`
//!       ←depends_on← `Solution: Dynamics 365 Finance`

use crate::entity::{Edge, EdgeKind, Entity, EntityKind, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct InMemoryGraph {
    nodes: HashMap<NodeId, Entity>,
    /// Adjacency: id → list of (neighbour, edge kind, weight)
    out_edges: HashMap<NodeId, Vec<(NodeId, EdgeKind, f32)>>,
    in_edges:  HashMap<NodeId, Vec<(NodeId, EdgeKind, f32)>>,
    /// Raw edge list for community-detection algorithms that prefer it.
    edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub by_kind: HashMap<String, usize>,
}

impl InMemoryGraph {
    pub fn new() -> Self { Self::default() }

    /// Seed with the cross-domain Dynamics 365 fixture set.  Idempotent.
    pub fn with_demo_corpus() -> Self {
        let mut g = Self::new();
        g.seed();
        g
    }

    pub fn add_entity(&mut self, e: Entity) {
        self.nodes.insert(e.id.clone(), e);
    }

    pub fn add_edge(&mut self, e: Edge) {
        self.out_edges.entry(e.from.clone()).or_default().push((e.to.clone(), e.kind, e.weight));
        self.in_edges .entry(e.to.clone())  .or_default().push((e.from.clone(), e.kind, e.weight));
        self.edges.push(e);
    }

    pub fn node(&self, id: &str) -> Option<&Entity> { self.nodes.get(id) }
    pub fn nodes(&self) -> impl Iterator<Item = &Entity> { self.nodes.values() }
    pub fn edges(&self) -> &[Edge] { &self.edges }

    /// Outgoing neighbours: `id → (to, kind, weight)`.
    pub fn outbound(&self, id: &str) -> &[(NodeId, EdgeKind, f32)] {
        self.out_edges.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Incoming neighbours.
    pub fn inbound(&self, id: &str) -> &[(NodeId, EdgeKind, f32)] {
        self.in_edges.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Undirected adjacency for community detection / PPR.
    pub fn undirected_neighbours(&self, id: &str) -> Vec<(NodeId, f32)> {
        let mut seen: HashMap<NodeId, f32> = HashMap::new();
        for (n, _, w) in self.outbound(id) { *seen.entry(n.clone()).or_insert(0.0) += w; }
        for (n, _, w) in self.inbound(id)  { *seen.entry(n.clone()).or_insert(0.0) += w; }
        seen.into_iter().collect()
    }

    pub fn stats(&self) -> GraphStats {
        let mut by_kind: HashMap<String, usize> = HashMap::new();
        for e in self.nodes.values() {
            *by_kind.entry(format!("{:?}", e.kind)).or_insert(0) += 1;
        }
        GraphStats {
            node_count: self.nodes.len(),
            edge_count: self.edges.len(),
            by_kind,
        }
    }

    /// Find nodes by free-text match over label + description + tags.
    /// Used by the HippoRAG seeding step.
    pub fn find_seeds(&self, query: &str, max_seeds: usize) -> Vec<NodeId> {
        let q = query.to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().filter(|t| t.len() >= 2).collect();
        if terms.is_empty() { return Vec::new(); }
        let mut scored: Vec<(usize, &Entity)> = self.nodes.values().filter_map(|e| {
            let hay = format!(
                "{} {} {}",
                e.label.to_lowercase(),
                e.description.as_deref().unwrap_or("").to_lowercase(),
                e.tags.join(" ").to_lowercase(),
            );
            let score: usize = terms.iter().map(|t| hay.matches(t).count()).sum();
            if score == 0 { None } else { Some((score, e)) }
        }).collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().take(max_seeds).map(|(_, e)| e.id.clone()).collect()
    }

    fn seed(&mut self) {
        let add = |g: &mut Self, id: &str, kind: EntityKind, label: &str, desc: &str, uri: Option<&str>, tags: &[&str]| {
            g.add_entity(Entity {
                id: id.into(), kind, label: label.into(),
                description: Some(desc.into()),
                uri: uri.map(String::from),
                tags: tags.iter().map(|s| s.to_string()).collect(),
            });
        };

        // X++ objects (model: GTFin)
        add(self, "xpp:GTFinPostJournal", EntityKind::XppObject, "GTFinPostJournal",
            "X++ job that posts general ledger journals via the LedgerGeneralJournalEntry OData service.",
            Some("xpp-obj://GTFin/GTFinPostJournal"), &["module:GeneralLedger", "model:GTFin", "kind:job"]);
        add(self, "xpp:GTFinJournalPoster", EntityKind::XppObject, "GTFinJournalPoster",
            "Helper class implementing GTFinPostable.",
            Some("xpp-obj://GTFin/GTFinJournalPoster"), &["module:GeneralLedger", "model:GTFin", "kind:class"]);
        add(self, "xpp:GTFinPostable", EntityKind::XppObject, "GTFinPostable",
            "GL posting contract interface.",
            Some("xpp-obj://GTFin/GTFinPostable"), &["module:GeneralLedger", "model:GTFin", "kind:interface"]);
        add(self, "xpp:GTFinConstants", EntityKind::XppObject, "GTFinConstants",
            "Shared constants for GTFin classes.",
            Some("xpp-obj://GTFin/GTFinConstants"), &["module:GeneralLedger", "model:GTFin", "kind:class"]);
        add(self, "xpp:GTFinValidation", EntityKind::XppObject, "GTFinValidation",
            "Validation routines for GL posting.",
            Some("xpp-obj://GTFin/GTFinValidation"), &["module:GeneralLedger", "model:GTFin", "kind:class"]);
        add(self, "xpp:GTScmGrnCheck", EntityKind::XppObject, "GTScmGrnCheck",
            "Product receipt reconciliation.",
            Some("xpp-obj://GTScm/GTScmGrnCheck"), &["module:SupplyChain", "model:GTScm", "kind:class"]);

        // OData services / Custom Service operations
        add(self, "svc:LedgerGeneralJournalEntry", EntityKind::Service, "LedgerGeneralJournalEntry",
            "Post a general ledger journal entry.",
            Some("d365-service://LedgerGeneralJournalEntry"), &["module:GeneralLedger", "group:Ledger"]);
        add(self, "svc:InventoryGoodsReceipt", EntityKind::Service, "InventoryGoodsReceipt",
            "Post a product (goods) receipt.",
            Some("d365-service://InventoryGoodsReceipt"), &["module:SupplyChain"]);
        add(self, "svc:ReleasedProductGetDetail", EntityKind::Service, "ReleasedProductGetDetail",
            "Read released product detail.",
            Some("d365-service://ReleasedProductGetDetail"), &["module:SupplyChain", "group:Product"]);
        add(self, "svc:SalesOrderCreate", EntityKind::Service, "SalesOrderCreate",
            "Create a sales order.",
            Some("d365-service://SalesOrderCreate"), &["module:Sales"]);

        // Data entities
        add(self, "ent:CompaniesV2", EntityKind::DataEntity, "CompaniesV2",
            "Legal entities / companies.", Some("d365-entity://CompaniesV2/structure"), &["module:GeneralLedger"]);
        add(self, "ent:FiscalCalendarPeriod", EntityKind::DataEntity, "FiscalCalendarPeriod",
            "Ledger fiscal periods.", Some("d365-entity://FiscalCalendarPeriod/structure"), &["module:GeneralLedger"]);
        add(self, "ent:ReleasedProductsV2", EntityKind::DataEntity, "ReleasedProductsV2",
            "Released products / item master.", Some("d365-entity://ReleasedProductsV2/structure"), &["module:SupplyChain"]);
        add(self, "ent:SalesOrderHeadersV2", EntityKind::DataEntity, "SalesOrderHeadersV2",
            "Sales order headers.", Some("d365-entity://SalesOrderHeadersV2/structure"), &["module:Sales"]);
        add(self, "ent:LedgerJournalTrans", EntityKind::DataEntity, "LedgerJournalTrans",
            "General journal lines.", Some("d365-entity://LedgerJournalTrans/structure"), &["module:GeneralLedger"]);
        add(self, "ent:GeneralJournalAccountEntry", EntityKind::DataEntity, "GeneralJournalAccountEntry",
            "Subledger general journal account entries (the universal accounting truth).",
            Some("d365-entity://GeneralJournalAccountEntry/structure"), &["module:GeneralLedger"]);

        // Power Automate flows / business processes
        add(self, "flow:P2P-001", EntityKind::Flow, "Procure-to-Pay (P2P)",
            "Purchase requisition through invoice verification.",
            Some("flow-proc://core/P2P-001"), &["process:p2p"]);
        add(self, "flow:O2C-002", EntityKind::Flow, "Order-to-Cash (O2C)",
            "Sales order through cash application.",
            Some("flow-proc://core/O2C-002"), &["process:o2c"]);

        // Solutions / modules
        add(self, "sol:FIN-CORE", EntityKind::Solution, "Dynamics 365 Finance",
            "Finance module running general ledger, AP, AR.",
            Some("solution://FIN-CORE"), &["lifecycle:active"]);
        add(self, "sol:LEGACY-BILL", EntityKind::Solution, "Legacy Billing Engine",
            "Phase-out billing engine.", Some("solution://LEGACY-BILL"), &["lifecycle:phase_out"]);

        // Microsoft Learn pages
        add(self, "learn:finance/period-close", EntityKind::LearnPage, "Financial period close in Dynamics 365 Finance",
            "Procedure for ledger period-end close.", Some("d365-learn://finance/period-close"), &["module:GeneralLedger"]);
        add(self, "learn:scm/product-receipt", EntityKind::LearnPage, "Product receipt posting",
            "Procedure for posting product receipts.", Some("d365-learn://scm/product-receipt"), &["module:SupplyChain"]);

        // Concepts (cross-domain hubs)
        add(self, "concept:period_close", EntityKind::Concept, "Period Close",
            "Financial period close: open/close ledger periods, foreign currency revaluation, reconciliation.",
            None, &["module:GeneralLedger"]);
        add(self, "concept:product_receipt", EntityKind::Concept, "Product Receipt",
            "Posting product receipts, issues, and transfers against the item master.",
            None, &["module:SupplyChain"]);
        add(self, "concept:journal_entry", EntityKind::Concept, "Journal Entry",
            "General journal posting that creates ledger documents.",
            None, &["module:GeneralLedger"]);

        // Edges
        let edges: Vec<(&str, &str, EdgeKind, f32)> = vec![
            // X++ class implements interface
            ("xpp:GTFinJournalPoster", "xpp:GTFinPostable", EdgeKind::Implements, 1.0),
            // Job uses class
            ("xpp:GTFinPostJournal", "xpp:GTFinJournalPoster", EdgeKind::Calls, 1.0),
            // Job uses shared constants + validation
            ("xpp:GTFinPostJournal", "xpp:GTFinConstants", EdgeKind::Uses, 1.0),
            ("xpp:GTFinPostJournal", "xpp:GTFinValidation", EdgeKind::Uses, 1.0),
            // Class + job call the OData service
            ("xpp:GTFinJournalPoster", "svc:LedgerGeneralJournalEntry", EdgeKind::Calls, 2.0),
            ("xpp:GTFinPostJournal",   "svc:LedgerGeneralJournalEntry", EdgeKind::Calls, 1.0),
            ("xpp:GTScmGrnCheck",      "svc:InventoryGoodsReceipt",     EdgeKind::Calls, 1.0),
            // Service reads / writes entities
            ("svc:LedgerGeneralJournalEntry", "ent:CompaniesV2",                EdgeKind::ReadsEntity, 1.0),
            ("svc:LedgerGeneralJournalEntry", "ent:FiscalCalendarPeriod",       EdgeKind::ReadsEntity, 1.0),
            ("svc:LedgerGeneralJournalEntry", "ent:LedgerJournalTrans",         EdgeKind::WritesEntity, 1.0),
            ("svc:LedgerGeneralJournalEntry", "ent:GeneralJournalAccountEntry", EdgeKind::WritesEntity, 1.0),
            ("svc:InventoryGoodsReceipt",     "ent:ReleasedProductsV2",         EdgeKind::ReadsEntity, 1.0),
            ("svc:SalesOrderCreate",          "ent:SalesOrderHeadersV2",        EdgeKind::WritesEntity, 1.0),
            // Flows depend on services
            ("flow:P2P-001", "svc:InventoryGoodsReceipt",     EdgeKind::DependsOn, 1.0),
            ("flow:P2P-001", "svc:LedgerGeneralJournalEntry", EdgeKind::DependsOn, 1.0),
            ("flow:O2C-002", "svc:SalesOrderCreate",          EdgeKind::DependsOn, 1.0),
            ("flow:O2C-002", "svc:LedgerGeneralJournalEntry", EdgeKind::DependsOn, 1.0),
            // Solutions depend on entities
            ("sol:FIN-CORE",    "ent:LedgerJournalTrans",         EdgeKind::DependsOn, 1.0),
            ("sol:FIN-CORE",    "ent:GeneralJournalAccountEntry", EdgeKind::DependsOn, 1.0),
            ("sol:FIN-CORE",    "ent:CompaniesV2",                EdgeKind::DependsOn, 1.0),
            ("sol:LEGACY-BILL", "ent:SalesOrderHeadersV2",        EdgeKind::DependsOn, 1.0),
            // Concepts describe entities (the cross-domain hubs)
            ("concept:period_close",    "learn:finance/period-close",     EdgeKind::Describes, 2.0),
            ("concept:period_close",    "ent:FiscalCalendarPeriod",       EdgeKind::Describes, 2.0),
            ("concept:period_close",    "ent:GeneralJournalAccountEntry", EdgeKind::Describes, 1.5),
            ("concept:period_close",    "sol:FIN-CORE",                   EdgeKind::Describes, 1.0),
            ("concept:journal_entry",   "svc:LedgerGeneralJournalEntry",  EdgeKind::Describes, 2.0),
            ("concept:journal_entry",   "ent:LedgerJournalTrans",         EdgeKind::Describes, 1.5),
            ("concept:journal_entry",   "xpp:GTFinPostJournal",           EdgeKind::Describes, 1.5),
            ("concept:product_receipt", "learn:scm/product-receipt",      EdgeKind::Describes, 2.0),
            ("concept:product_receipt", "svc:InventoryGoodsReceipt",      EdgeKind::Describes, 2.0),
            ("concept:product_receipt", "xpp:GTScmGrnCheck",              EdgeKind::Describes, 1.0),
            // Learn pages reference entities / services
            ("learn:finance/period-close", "ent:FiscalCalendarPeriod",       EdgeKind::References, 1.0),
            ("learn:finance/period-close", "ent:GeneralJournalAccountEntry", EdgeKind::References, 1.0),
            ("learn:scm/product-receipt",  "svc:InventoryGoodsReceipt",      EdgeKind::References, 1.0),
        ];
        let mut seen: HashSet<(NodeId, NodeId, EdgeKind)> = HashSet::new();
        for (from, to, kind, weight) in edges {
            let key = (from.to_string(), to.to_string(), kind);
            if seen.insert(key) {
                self.add_edge(Edge {
                    from: from.into(), to: to.into(), kind, weight,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_corpus_has_cross_domain_edges() {
        let g = InMemoryGraph::with_demo_corpus();
        let stats = g.stats();
        assert!(stats.node_count >= 20, "expected >= 20 nodes, got {}", stats.node_count);
        assert!(stats.edge_count >= 25, "expected >= 25 edges, got {}", stats.edge_count);
        // The period_close concept should reach the FIN-CORE solution in two hops:
        // concept:period_close → ent:GeneralJournalAccountEntry ← sol:FIN-CORE
        assert!(g.outbound("concept:period_close").iter().any(|(n, _, _)| n == "ent:GeneralJournalAccountEntry"));
        assert!(g.inbound("ent:GeneralJournalAccountEntry").iter().any(|(n, _, _)| n == "sol:FIN-CORE"));
    }

    #[test]
    fn find_seeds_locates_relevant_entities() {
        let g = InMemoryGraph::with_demo_corpus();
        let seeds = g.find_seeds("period close GeneralJournalAccountEntry", 5);
        assert!(seeds.iter().any(|s| s == "concept:period_close" || s == "ent:GeneralJournalAccountEntry" || s == "learn:finance/period-close"));
    }
}
