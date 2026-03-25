use anyhow::Result;
use async_trait::async_trait;

use super::provider::EmbeddingProvider;

/// A local, deterministic embedding provider that requires no external API.
///
/// Uses character n-gram hashing to produce fixed-dimension vectors. This is
/// significantly less accurate than neural embeddings (OpenAI, etc.) but works
/// offline, has zero latency, and is useful for:
/// - Air-gapped or self-hosted deployments without API access
/// - Development and testing without burning API credits
/// - Fallback when the primary embedding provider is unavailable
///
/// The algorithm hashes overlapping character trigrams into a fixed-size vector
/// and L2-normalizes the result. Cosine similarity between these vectors
/// correlates with surface-level text similarity (shared substrings/terms).
pub struct LocalEmbeddingProvider {
    dimensions: usize,
}

impl LocalEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }

    /// Default configuration: 384-dimensional vectors (a common small embedding size).
    pub fn default_384() -> Self {
        Self::new(384)
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(hash_embed(text, self.dimensions))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| hash_embed(t, self.dimensions)).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn name(&self) -> &str {
        "local/ngram-hash"
    }
}

/// Produce a deterministic embedding by hashing character n-grams into buckets.
///
/// Steps:
/// 1. Lowercase and tokenize the input.
/// 2. For each character trigram, compute a hash and map it to a vector dimension.
/// 3. Accumulate counts (like a bag-of-ngrams).
/// 4. L2-normalize the result so cosine similarity is meaningful.
fn hash_embed(text: &str, dimensions: usize) -> Vec<f32> {
    let mut vector = vec![0.0f32; dimensions];

    let normalized: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
        .collect();

    let chars: Vec<char> = normalized.chars().collect();

    if chars.len() < 3 {
        // For very short text, hash the whole thing.
        let h = fnv1a(normalized.as_bytes());
        let idx = (h as usize) % dimensions;
        vector[idx] = 1.0;
    } else {
        // Hash each trigram into a bucket.
        for window in chars.windows(3) {
            let trigram: String = window.iter().collect();
            let h = fnv1a(trigram.as_bytes());
            let idx = (h as usize) % dimensions;
            // Use a second hash to determine sign (reduces collision impact).
            let sign = if (h >> 32) & 1 == 0 { 1.0 } else { -1.0 };
            vector[idx] += sign;
        }
    }

    // L2 normalize.
    let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vector {
            *v /= norm;
        }
    }

    vector
}

/// FNV-1a hash — fast, non-cryptographic, good distribution for bucketing.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_embedding_produces_correct_dimensions() {
        let provider = LocalEmbeddingProvider::new(128);
        let embedding = provider.embed("test input text").await.unwrap();
        assert_eq!(embedding.len(), 128);
    }

    #[tokio::test]
    async fn local_embedding_is_deterministic() {
        let provider = LocalEmbeddingProvider::new(64);
        let a = provider.embed("monthly revenue by region").await.unwrap();
        let b = provider.embed("monthly revenue by region").await.unwrap();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn local_embedding_is_normalized() {
        let provider = LocalEmbeddingProvider::new(256);
        let embedding = provider.embed("some longer test text for embedding").await.unwrap();
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "expected unit norm, got {norm}");
    }

    #[tokio::test]
    async fn similar_texts_have_higher_similarity() {
        let provider = LocalEmbeddingProvider::new(384);
        let a = provider.embed("total revenue by customer").await.unwrap();
        let b = provider.embed("total revenue by region").await.unwrap();
        let c = provider.embed("completely unrelated quantum physics").await.unwrap();

        let sim_ab: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let sim_ac: f32 = a.iter().zip(c.iter()).map(|(x, y)| x * y).sum();

        assert!(sim_ab > sim_ac, "similar texts should have higher cosine similarity: {sim_ab} vs {sim_ac}");
    }

    #[tokio::test]
    async fn batch_embedding_works() {
        let provider = LocalEmbeddingProvider::new(64);
        let texts = &["hello world", "foo bar", "test"];
        let results = provider.embed_batch(texts).await.unwrap();
        assert_eq!(results.len(), 3);
        for r in &results {
            assert_eq!(r.len(), 64);
        }
    }
}
