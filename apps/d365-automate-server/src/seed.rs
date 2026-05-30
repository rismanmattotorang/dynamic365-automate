//! Seed corpus: small, illustrative documents across the four Dynamics 365
//! knowledge domains (X++, Power Automate flows, Dataverse solutions, Learn).
//!
//! Runs the documents through the chunker and embedder so the KnowledgeStore
//! exposes the same chunk-level surface as a real ingestion pipeline.

use d365_automate_ingest::{chunk_document, ChunkerConfig, EmbeddingClient};
use d365_automate_kb::{Document, Domain, KnowledgeStore, UpsertBatch};

pub async fn populate_with_embeddings(
    store: &std::sync::Arc<dyn KnowledgeStore>,
    embedder: &dyn EmbeddingClient,
) -> anyhow::Result<()> {
    let docs = seed_documents();
    let chunker = ChunkerConfig::default();

    for doc in docs {
        let mut chunks = chunk_document(&doc, &chunker);
        if chunks.is_empty() { continue; }
        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
        let vectors = embedder.embed(&texts).await?;
        for (chunk, vec) in chunks.iter_mut().zip(vectors.into_iter()) {
            chunk.embedding = Some(vec);
        }
        store.upsert(UpsertBatch { documents: vec![doc], chunks }).await?;
    }
    Ok(())
}

fn seed_documents() -> Vec<Document> {
    let mut out = Vec::new();

    out.push({
        let mut d = Document::new(
            "xpp:GTFin/GTFinPostJournal", Domain::Xpp, "xpp-obj://GTFin/GTFinPostJournal",
            "GTFinPostJournal",
            "X++ job GTFinPostJournal posts general ledger journals via the LedgerGeneralJournalEntry \
             OData service. Validates the ledger period is open (FiscalCalendarPeriod), checks financial \
             dimensions, and submits the journal lines in a $batch change set so the post is atomic.",
        );
        d.metadata.insert("model".into(), "GTFin".into());
        d.metadata.insert("type".into(), "JOB".into());
        d
    });

    out.push({
        let mut d = Document::new(
            "xpp:GTScm/GTScmGrnCheck", Domain::Xpp, "xpp-obj://GTScm/GTScmGrnCheck",
            "GTScmGrnCheck",
            "X++ class GTScmGrnCheck reconciles product (goods) receipts with purchase order quantities; \
             triggers tolerance checks and inventory valuation flow. Calls the InventoryProductReceiptPost \
             OData operation on success.",
        );
        d.metadata.insert("model".into(), "GTScm".into());
        d.metadata.insert("type".into(), "CLASS".into());
        d
    });

    out.push({
        let mut d = Document::new(
            "flow:core/P2P-001", Domain::Flow, "flow-proc://core/P2P-001",
            "Procure-to-Pay (P2P)",
            "Power Automate / business process P2P-001: purchase requisition → PO approval → product \
             receipt → invoice matching → payment release. Throughput drops 18% at PO approval due to \
             approver coverage gaps.",
        );
        d.breadcrumbs = vec!["core".into()];
        d
    });

    out.push({
        let mut d = Document::new(
            "flow:core/O2C-002", Domain::Flow, "flow-proc://core/O2C-002",
            "Order-to-Cash (O2C)",
            "Power Automate / business process O2C-002: sales order entry → availability check → delivery \
             → invoicing → settlement. Mining shows a 12% rework loop between invoicing and delivery, \
             primarily caused by incomplete delivery addresses.",
        );
        d.breadcrumbs = vec!["core".into()];
        d
    });

    out.push(Document::new(
        "solution:FIN-CORE", Domain::Solution, "solution://FIN-CORE",
        "Dynamics 365 Finance",
        "Solution fact sheet for the Dynamics 365 Finance module (FIN-CORE). Lifecycle: active. \
         Capabilities: general ledger, accounts payable, accounts receivable, fixed assets. \
         Integrations: GTFinPostJournal, dual-write to Dataverse. EOL: 2031-12.",
    ));

    out.push(Document::new(
        "solution:LEGACY-BILL", Domain::Solution, "solution://LEGACY-BILL",
        "Legacy Billing Engine",
        "Solution fact sheet for the Legacy Billing Engine (LEGACY-BILL). Lifecycle: phase-out. \
         Integrations: mainframe. EOL: 2026-09. Replacement: Dynamics 365 Sales/Finance billing.",
    ));

    out.push({
        let mut d = Document::new(
            "learn:finance/period-close", Domain::Learn, "d365-learn://finance/period-close",
            "Financial period close in Dynamics 365 Finance",
            "Microsoft Learn page on financial period close: open and close ledger periods via the \
             FiscalCalendarPeriod entity, execute foreign-currency revaluation, post accruals and \
             deferrals, run LedgerJournalTrans to GeneralJournalAccountEntry reconciliation, and generate \
             the balance audit trail.",
        );
        d.breadcrumbs = vec!["Finance".into(), "General Ledger".into()];
        d.metadata.insert("module".into(), "GeneralLedger".into());
        d
    });

    out.push({
        let mut d = Document::new(
            "learn:scm/product-receipt", Domain::Learn, "d365-learn://scm/product-receipt",
            "Product receipt posting",
            "Microsoft Learn page describing product receipt posting against a purchase order. \
             Journal types: arrival receipt, reversal, return. Updates InventTrans and the inventory \
             valuation.",
        );
        d.breadcrumbs = vec!["Supply Chain".into(), "Inventory Management".into()];
        d.metadata.insert("module".into(), "SupplyChain".into());
        d
    });

    out
}
