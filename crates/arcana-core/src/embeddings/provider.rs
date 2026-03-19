use anyhow::Result;
use async_trait::async_trait;

/// A provider that converts text into embedding vectors.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single piece of text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in one batch (implementations may parallelize or batch the requests).
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// The dimensionality of the vectors produced by this provider.
    fn dimensions(&self) -> usize;

    /// A human-readable name for this provider (e.g., "openai/text-embedding-3-small").
    fn name(&self) -> &str;
}
