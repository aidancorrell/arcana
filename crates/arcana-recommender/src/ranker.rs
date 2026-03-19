use anyhow::Result;
use arcana_core::{
    embeddings::{EmbeddingProvider, VectorIndex},
    entities::{Column, DocumentChunk, SemanticDefinition, Table},
    store::MetadataStore,
};
use std::sync::Arc;
use uuid::Uuid;

/// A request for context from an AI agent.
#[derive(Debug, Clone)]
pub struct ContextRequest {
    /// The natural-language query from the agent.
    pub query: String,
    /// Maximum number of results to return.
    pub top_k: usize,
    /// If set, only return results for this specific table.
    pub filter_table_id: Option<Uuid>,
    /// Minimum confidence score to include in results.
    pub min_confidence: f64,
}

impl Default for ContextRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            top_k: 10,
            filter_table_id: None,
            min_confidence: 0.0,
        }
    }
}

/// A single ranked context item returned to the agent.
#[derive(Debug, Clone)]
pub struct ContextItem {
    pub entity_id: Uuid,
    pub entity_type: ContextEntityType,
    pub relevance_score: f64,
    pub confidence: f64,
    /// Human-readable label for this item.
    pub label: String,
    /// Serialized content to include in the context response.
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextEntityType {
    Table,
    Column,
    SemanticDefinition,
    DocumentChunk,
}

/// The ranked set of context items to return to the agent.
#[derive(Debug, Default)]
pub struct ContextResult {
    pub items: Vec<ContextItem>,
    /// Total tokens estimated for this context (rough: 1 token ≈ 4 chars).
    pub estimated_tokens: usize,
}

/// Scores and ranks metadata entities for relevance to an agent's query.
pub struct RelevanceRanker {
    store: Arc<dyn MetadataStore>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    entity_index: Arc<VectorIndex>,
    chunk_index: Arc<VectorIndex>,
}

impl RelevanceRanker {
    pub fn new(
        store: Arc<dyn MetadataStore>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        entity_index: Arc<VectorIndex>,
        chunk_index: Arc<VectorIndex>,
    ) -> Self {
        Self {
            store,
            embedding_provider,
            entity_index,
            chunk_index,
        }
    }

    /// Rank context items for a given request.
    pub async fn rank(&self, request: &ContextRequest) -> Result<ContextResult> {
        // 1. Embed the query
        let query_embedding = self.embedding_provider.embed(&request.query).await?;

        // 2. Search entity index (tables, columns, semantic definitions)
        let entity_hits = self
            .entity_index
            .search(&query_embedding, request.top_k * 2)?;

        // 3. Search chunk index (document chunks)
        let chunk_hits = self
            .chunk_index
            .search(&query_embedding, request.top_k)?;

        // 4. Merge, deduplicate, and re-rank by combined score
        // TODO: fetch entity metadata for each hit from the store,
        //       apply confidence decay, and compute combined relevance score.
        let _ = (entity_hits, chunk_hits);

        Ok(ContextResult {
            items: vec![],
            estimated_tokens: 0,
        })
    }

    /// Compute a combined relevance score from semantic similarity and confidence.
    ///
    /// Formula: `relevance * 0.7 + confidence * 0.3`
    pub fn combined_score(semantic_similarity: f64, confidence: f64) -> f64 {
        (semantic_similarity * 0.7 + confidence * 0.3).clamp(0.0, 1.0)
    }
}
