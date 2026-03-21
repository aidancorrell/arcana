use anyhow::Result;
use arcana_core::{
    confidence::ConfidenceDecay,
    embeddings::{EmbeddingProvider, VectorIndex},
    entities::{Column, DocumentChunk, LineageEdge, SemanticDefinition, Table},
    store::MetadataStore,
};
use std::collections::{HashMap, HashSet};
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
    /// Include upstream lineage tables in the context response.
    pub expand_lineage: bool,
}

impl Default for ContextRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            top_k: 10,
            filter_table_id: None,
            min_confidence: 0.0,
            expand_lineage: false,
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
    /// Lineage edges included when `expand_lineage` is true.
    pub lineage_edges: Vec<LineageEdge>,
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

    /// Rank context items for a given request using hybrid BM25 + dense retrieval.
    ///
    /// Stage 1 — retrieval: FTS5 BM25 and dense cosine search each return a
    ///   candidate pool of `top_k * 4` results.
    /// Stage 2 — fusion: Reciprocal Rank Fusion (RRF, k=60) merges both ranked
    ///   lists into a single fused ranking without requiring score calibration.
    /// Stage 3 — scoring: fetch each candidate entity, apply confidence decay,
    ///   and compute `semantic_similarity * 0.7 + confidence * 0.3`.
    pub async fn rank(&self, request: &ContextRequest) -> Result<ContextResult> {
        let decay = ConfidenceDecay::default();
        let candidate_n = request.top_k * 4;

        // 1. Parallel candidate retrieval: FTS (BM25) + dense (cosine)
        let query_embedding = self.embedding_provider.embed(&request.query).await?;

        let fts_hits = self.store.fts_search(&request.query, candidate_n as u32).await?;
        let vector_hits = self.entity_index.search(&query_embedding, candidate_n)?;

        // 2. Reciprocal Rank Fusion — produces a fused ranked list of entity_ids
        let fused = rrf_fuse(&fts_hits, &vector_hits);

        // 3. Search chunk index separately (document chunks aren't in FTS)
        let chunk_hits = self.chunk_index.search(&query_embedding, request.top_k)?;

        // 4. For each fused hit: fetch entity, apply confidence decay, compute score
        let mut items: Vec<ContextItem> = Vec::new();

        for (entity_id, rrf_score) in &fused {
            // Try as a table first
            if let Some(table) = self.store.get_table(*entity_id).await? {
                if let Some(filter_id) = request.filter_table_id {
                    if table.id != filter_id {
                        continue;
                    }
                }

                let decayed = decay.decayed_score(table.confidence, table.confidence_refreshed_at);
                let score = Self::combined_score(*rrf_score, decayed.value());

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

            // Not a table — check for semantic definitions (column or metric)
            let definitions = self.store.get_semantic_definitions(*entity_id).await?;
            if let Some(best_def) = definitions.first() {
                let decayed =
                    decay.decayed_score(best_def.confidence, best_def.confidence_refreshed_at);
                let score = Self::combined_score(*rrf_score, decayed.value());

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

        // Process chunk hits (document chunks bypass FTS/RRF — pure dense search)
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

        // 6. Lineage expansion: for each table hit, fetch upstream tables
        let mut lineage_edges = Vec::new();
        if request.expand_lineage {
            let mut upstream_ids: HashSet<Uuid> = HashSet::new();
            for item in &items {
                if item.entity_type == ContextEntityType::Table {
                    if let Ok(edges) = self.store.get_upstream(item.entity_id).await {
                        for edge in &edges {
                            upstream_ids.insert(edge.upstream_id);
                        }
                        lineage_edges.extend(edges);
                    }
                }
            }

            // Add upstream tables that aren't already in results (within token budget)
            let existing_ids: HashSet<Uuid> = items.iter().map(|i| i.entity_id).collect();
            for upstream_id in upstream_ids {
                if existing_ids.contains(&upstream_id) {
                    continue;
                }
                if let Ok(Some(table)) = self.store.get_table(upstream_id).await {
                    let decayed = decay.decayed_score(table.confidence, table.confidence_refreshed_at);
                    items.push(ContextItem {
                        entity_id: table.id,
                        entity_type: ContextEntityType::Table,
                        relevance_score: 0.0, // lineage-injected, not search-ranked
                        confidence: decayed.value(),
                        label: format!("[upstream] {}", table.name),
                        content: table.description.clone().unwrap_or_default(),
                    });
                }
            }
        }

        // 7. Non-canonical warning: flag tables that belong to a cluster but aren't canonical
        for item in &mut items {
            if item.entity_type == ContextEntityType::Table {
                if let Ok(Some((cluster, _members))) = self.store.get_cluster_for_table(item.entity_id).await {
                    if let Some(canonical_id) = cluster.canonical_id {
                        if canonical_id != item.entity_id {
                            if let Ok(Some(canonical_table)) = self.store.get_table(canonical_id).await {
                                item.label = format!(
                                    "{} [⚠ non-canonical — prefer {}]",
                                    item.label, canonical_table.name
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(ContextResult {
            items,
            estimated_tokens,
            lineage_edges,
        })
    }

    /// Compute a combined relevance score from semantic similarity and confidence.
    ///
    /// Formula: `relevance * 0.7 + confidence * 0.3`
    pub fn combined_score(semantic_similarity: f64, confidence: f64) -> f64 {
        (semantic_similarity * 0.7 + confidence * 0.3).clamp(0.0, 1.0)
    }
}

/// Reciprocal Rank Fusion — merges two ranked lists without score calibration.
///
/// RRF score = Σ 1/(k + rank_i) where k=60 is the standard smoothing constant.
/// Scores are normalized to [0, 1] relative to the best hit so they compose
/// correctly with the confidence term in `combined_score`.
/// Higher score = better. Entities appearing in both lists are rewarded.
fn rrf_fuse(
    fts_hits: &[(Uuid, f32)],
    vector_hits: &[(Uuid, f32)],
) -> Vec<(Uuid, f64)> {
    const K: f64 = 60.0;
    let mut scores: HashMap<Uuid, f64> = HashMap::new();

    for (rank, (id, _)) in fts_hits.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (K + rank as f64 + 1.0);
    }
    for (rank, (id, _)) in vector_hits.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (K + rank as f64 + 1.0);
    }

    let mut fused: Vec<(Uuid, f64)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Normalize to [0, 1] so scores compose correctly with the confidence term
    if let Some(&(_, max)) = fused.first() {
        if max > 0.0 {
            for (_, s) in &mut fused {
                *s /= max;
            }
        }
    }

    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_rewards_overlap() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();

        // a appears in both lists at rank 0; b only in fts; c only in vector
        let fts = vec![(a, 1.0f32), (b, 0.8f32)];
        let vec = vec![(a, 1.0f32), (c, 0.8f32)];

        let fused = rrf_fuse(&fts, &vec);
        let score_of = |id: Uuid| fused.iter().find(|(i, _)| *i == id).map(|(_, s)| *s).unwrap_or(0.0);

        // a appears in both lists — must outscore b and c which appear in only one
        assert!(score_of(a) > score_of(b), "overlap should boost a above b");
        assert!(score_of(a) > score_of(c), "overlap should boost a above c");
    }

    #[test]
    fn rrf_empty_lists() {
        let fused = rrf_fuse(&[], &[]);
        assert!(fused.is_empty());
    }
}
