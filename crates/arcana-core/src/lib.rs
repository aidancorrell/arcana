//! # arcana-core
//!
//! Core library for the Arcana agent-first data catalog. Provides the metadata
//! store trait, entity types, embedding index, confidence system, and enrichment
//! provider abstractions.

/// Confidence scoring with time-based decay.
pub mod confidence;
/// Embedding providers and in-memory vector index.
pub mod embeddings;
/// LLM-based enrichment for generating semantic definitions.
pub mod enrichment;
/// All entity types: tables, columns, schemas, definitions, lineage, contracts, etc.
pub mod entities;
/// The `MetadataStore` trait and SQLite implementation.
pub mod store;

pub use confidence::{ConfidenceScore, ConfidenceDecay};
pub use entities::*;

/// SHA-256 hash of a definition string, used for embedding cache.
pub fn definition_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}
