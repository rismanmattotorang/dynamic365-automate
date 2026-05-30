//! D365-Automate ingestion pipeline.
//!
//! Phase 1A scope (paper §X-B):
//!   - HTML extraction for the Microsoft Learn page model
//!   - chunker that prepends a normalised breadcrumb (paper §VI-E)
//!   - pluggable `EmbeddingClient` with a deterministic mock + OpenAI adapter
//!   - `IngestionPipeline` orchestrator: crawl -> parse -> chunk -> embed ->
//!     upsert

pub mod chunker;
pub mod crawler;
pub mod embed;
pub mod fit_markdown;
pub mod pipeline;
pub mod rate_limit;
pub mod robots;

pub use chunker::{chunk_document, ChunkerConfig};
pub use crawler::{LearnCrawler, ParsedPage, parse_learn_html};
pub use embed::{EmbeddingClient, EmbeddingError, MockEmbedder, OpenAiEmbedder};
pub use fit_markdown::{fit_markdown_filter, FitConfig, FitStats};
pub use pipeline::{IngestionPipeline, IngestionReport};
pub use rate_limit::RateLimiter;
pub use robots::{Decision, RobotsTxt};
