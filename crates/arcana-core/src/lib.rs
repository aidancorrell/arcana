pub mod confidence;
pub mod embeddings;
pub mod enrichment;
pub mod entities;
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
