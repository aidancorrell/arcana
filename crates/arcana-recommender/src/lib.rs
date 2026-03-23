//! # arcana-recommender
//!
//! Context recommendation engine. Given a natural-language query, ranks and
//! serializes the most relevant metadata (tables, columns, definitions, lineage)
//! within a configurable token budget.

/// Redundancy detection — semantic clustering of similar tables.
pub mod dedup;
/// Agent interaction logging and feedback processing.
pub mod feedback;
/// Hybrid search ranker (BM25 + dense cosine, fused via RRF).
pub mod ranker;
/// Token-budget-aware serializer (Markdown, JSON, Prose formats).
pub mod serializer;

pub use ranker::{ContextRequest, ContextResult, RelevanceRanker};
pub use serializer::ContextSerializer;
