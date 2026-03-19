use anyhow::Result;
use arcana_core::{
    confidence::ConfidenceDecay,
    embeddings::{EmbeddingProvider, VectorIndex},
    entities::{Column, DocumentChunk, SemanticDefinition, Table},
    store::MetadataStore,
};
use std::collections::HashSet;
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
        let decay = ConfidenceDecay::default();

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

        // 4. For each hit: fetch entity, apply confidence decay, compute combined score
        let mut items: Vec<ContextItem> = Vec::new();

        for (entity_id, similarity) in &entity_hits {
            let similarity_f64 = *similarity as f64;

            // Try as a table first
            if let Some(table) = self.store.get_table(*entity_id).await? {
                if let Some(filter_id) = request.filter_table_id {
                    if table.id != filter_id {
                        continue;
                    }
                }

                let decayed = decay.decayed_score(table.confidence, table.confidence_refreshed_at);
                let score = Self::combined_score(similarity_f64, decayed.value());

                if score < request.min_confidence {
                    continue;
                }

                let definitions = self.store.get_semantic_definitions(table.id).await?;
                let content = definitions
                    .first()
                    .map(|d| d.definition.clone())
                    .or_else(|| table.description.clone())
                    .unwrap_or_default();

                items.push(ContextItem {
                    entity_id: table.id,
                    entity_type: ContextEntityType::Table,
                    relevance_score: score,
                    confidence: decayed.value(),
                    label: table.name.clone(),
                    content,
                });
                continue;
            }

            // Not a table — check for semantic definitions (could be column or metric)
            let definitions = self.store.get_semantic_definitions(*entity_id).await?;
            if let Some(best_def) = definitions.first() {
                let decayed =
                    decay.decayed_score(best_def.confidence, best_def.confidence_refreshed_at);
                let score = Self::combined_score(similarity_f64, decayed.value());

                if score < request.min_confidence {
                    continue;
                }

                use arcana_core::entities::SemanticEntityType;
                let entity_type = match best_def.entity_type {
                    SemanticEntityType::Table => ContextEntityType::Table,
                    SemanticEntityType::Column => ContextEntityType::Column,
                    SemanticEntityType::Metric => ContextEntityType::SemanticDefinition,
                };

                items.push(ContextItem {
                    entity_id: *entity_id,
                    entity_type,
                    relevance_score: score,
                    confidence: decayed.value(),
                    label: best_def
                        .definition
                        .chars()
                        .take(80)
                        .collect::<String>(),
                    content: best_def.definition.clone(),
                });
            }
        }

        // Process chunk hits
        for (chunk_id, similarity) in &chunk_hits {
            let score = *similarity as f64;
            if score < request.min_confidence {
                continue;
            }

            items.push(ContextItem {
                entity_id: *chunk_id,
                entity_type: ContextEntityType::DocumentChunk,
                relevance_score: score,
                confidence: score,
                label: format!("doc-chunk:{}", &chunk_id.to_string()[..8]),
                content: String::new(),
            });
        }

        // 5. Deduplicate by entity_id (keep highest score), sort descending, truncate
        items.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut seen = HashSet::new();
        items.retain(|item| seen.insert(item.entity_id));

        items.truncate(request.top_k);

        let estimated_tokens = items.iter().map(|i| i.content.len() / 4 + 20).sum();

        Ok(ContextResult {
            items,
            estimated_tokens,
        })
    }

    /// Compute a combined relevance score from semantic similarity and confidence.
    ///
    /// Formula: `relevance * 0.7 + confidence * 0.3`
    pub fn combined_score(semantic_similarity: f64, confidence: f64) -> f64 {
        (semantic_similarity * 0.7 + confidence * 0.3).clamp(0.0, 1.0)
    }
}
