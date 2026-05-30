//! D365-Automate knowledge base.
//!
//! Phase 1A: introduces the `KnowledgeStore` trait so backends are pluggable.
//! Two implementations ship: `InMemoryKb` (dev/test) and `QdrantStore`
//! (production, behind the `qdrant` feature).  Both implement the same
//! `KnowledgeStore` async contract so the RAG engine and ingestion pipeline
//! see one surface.

pub mod doc_tree;
pub mod schema;
pub mod store;

#[cfg(feature = "qdrant")]
pub mod qdrant;

pub use doc_tree::{build_document_tree, DocTreeNode, DocumentTree};
pub use schema::{content_hash, Chunk, ChunkId, Document, DocumentId, Domain};
pub use store::{
    InMemoryKb, KnowledgeStore, Layer, SearchHit, SearchQuery, StoreError, UpsertBatch, UpsertStats,
};

#[cfg(feature = "qdrant")]
pub use qdrant::QdrantStore;
