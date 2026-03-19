use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::provider::EmbeddingProvider;

/// OpenAI Embeddings API client.
pub struct OpenAiEmbeddingProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    dimensions: usize,
}

impl OpenAiEmbeddingProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>, dimensions: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            dimensions,
        }
    }

    /// Convenience constructor for `text-embedding-3-small` (1536 dims).
    pub fn text_embedding_3_small(api_key: impl Into<String>) -> Self {
        Self::new(api_key, "text-embedding-3-small", 1536)
    }
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let response = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&EmbedRequest {
                model: &self.model,
                input: text,
            })
            .send()
            .await
            .context("failed to send embedding request to OpenAI")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI embeddings API returned {status}: {body}");
        }

        let parsed: EmbedResponse = response
            .json()
            .await
            .context("failed to parse OpenAI embedding response")?;

        parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .context("OpenAI returned empty embedding data array")
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn name(&self) -> &str {
        &self.model
    }
}
