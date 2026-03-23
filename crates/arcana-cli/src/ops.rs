//! Shared operations for CLI commands and background sync.
//!
//! Extracts common enrich/reembed logic so `cmd_enrich`, `cmd_reembed`,
//! and `run_background_sync` all call into the same code paths.

use anyhow::Result;
use arcana_core::embeddings::EmbeddingProvider;
use arcana_core::enrichment::{EnrichmentProvider, EnrichmentRequest};
use arcana_core::entities::{DefinitionSource, SemanticDefinition, SemanticEntityType};
use arcana_core::store::MetadataStore;
use uuid::Uuid;

/// Write enrichment results back to the store.
///
/// If `dry_run` is true, prints what would be written but makes no changes.
/// Returns the number of definitions written (or that would be written).
pub async fn write_enrichment_batch(
    store: &dyn MetadataStore,
    provider: &dyn EnrichmentProvider,
    requests: &[EnrichmentRequest],
    entity_ids: &[Uuid],
    dry_run: bool,
) -> Result<usize> {
    let responses = provider.enrich_batch(requests).await?;
    let mut written = 0usize;

    for (entity_id, (req, resp)) in entity_ids.iter().zip(requests.iter().zip(responses.iter())) {
        if resp.definition.is_empty() {
            continue;
        }

        let entity_type = if req.column_name.is_some() {
            SemanticEntityType::Column
        } else {
            SemanticEntityType::Table
        };

        let label = req
            .column_name
            .as_deref()
            .unwrap_or(&req.table_name);

        if dry_run {
            println!(
                "  [dry-run] {label}: {}",
                &resp.definition[..resp.definition.len().min(80)]
            );
        } else {
            let now = chrono::Utc::now();
            let def = SemanticDefinition {
                id: Uuid::new_v4(),
                entity_id: *entity_id,
                entity_type,
                definition: resp.definition.clone(),
                source: DefinitionSource::LlmInferred,
                confidence: 0.40,
                embedding: None,
                definition_hash: None,
                confidence_refreshed_at: Some(now),
                created_at: now,
                updated_at: now,
            };
            store.upsert_semantic_definition(&def).await?;
            tracing::debug!("enriched {label}");
        }
        written += 1;
    }

    Ok(written)
}

/// Collect enrichment requests for undescribed tables in the store.
///
/// Returns `(requests, entity_ids)` for all tables that lack a human-edited
/// or adapter-sourced definition, optionally filtered by name substring.
pub async fn collect_table_enrichment_targets(
    store: &dyn MetadataStore,
    filter: Option<&str>,
) -> Result<(Vec<EnrichmentRequest>, Vec<Uuid>, usize)> {
    let mut requests = Vec::new();
    let mut ids = Vec::new();
    let mut skipped = 0usize;

    let data_sources = store.list_data_sources().await?;
    for ds in &data_sources {
        let schemas = store.list_schemas(ds.id).await?;
        for schema in &schemas {
            let tables = store.list_tables(schema.id).await?;
            for table in &tables {
                if let Some(f) = filter {
                    if !table.name.contains(f) {
                        skipped += 1;
                        continue;
                    }
                }

                let existing = store.get_semantic_definitions(table.id).await?;
                if has_authoritative_definition(&existing) {
                    skipped += 1;
                    continue;
                }

                let cols = store.list_columns(table.id).await?;
                let col_names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
                let upstream_edges = store.get_upstream(table.id).await?;
                let upstream_tables: Vec<String> = upstream_edges
                    .iter()
                    .map(|e| e.upstream_id.to_string())
                    .collect();

                requests.push(EnrichmentRequest {
                    table_name: table.name.clone(),
                    column_names: col_names,
                    upstream_tables,
                    column_name: None,
                });
                ids.push(table.id);
            }
        }
    }

    Ok((requests, ids, skipped))
}

/// Re-embed all semantic definitions that need it.
///
/// Skips definitions where the text hasn't changed (based on definition_hash)
/// and an embedding already exists. Returns the number re-embedded.
pub async fn reembed_definitions(
    store: &dyn MetadataStore,
    provider: &dyn EmbeddingProvider,
    below_confidence: Option<f64>,
    batch_size: usize,
) -> Result<usize> {
    let threshold = below_confidence.unwrap_or(f64::MAX);
    let all_defs = store.list_all_semantic_definitions().await?;
    let mut count = 0usize;

    let mut batch_texts: Vec<String> = Vec::new();
    let mut batch_defs: Vec<SemanticDefinition> = Vec::new();

    for def in all_defs {
        if def.confidence >= threshold {
            continue;
        }

        // Embedding cache: skip if definition text unchanged and embedding exists
        let hash = arcana_core::definition_hash(&def.definition);
        if def.embedding.is_some() && def.definition_hash.as_deref() == Some(hash.as_str()) {
            continue;
        }

        batch_texts.push(def.definition.clone());
        batch_defs.push(def);

        if batch_texts.len() >= batch_size {
            count += flush_embed_batch(store, provider, &mut batch_texts, &mut batch_defs).await?;
        }
    }

    // Flush remaining
    if !batch_texts.is_empty() {
        count += flush_embed_batch(store, provider, &mut batch_texts, &mut batch_defs).await?;
    }

    Ok(count)
}

async fn flush_embed_batch(
    store: &dyn MetadataStore,
    provider: &dyn EmbeddingProvider,
    texts: &mut Vec<String>,
    defs: &mut Vec<SemanticDefinition>,
) -> Result<usize> {
    let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    let embeddings = provider.embed_batch(&refs).await?;
    let mut count = 0;
    for (mut d, emb) in defs.drain(..).zip(embeddings.into_iter()) {
        d.embedding = Some(emb);
        d.definition_hash = Some(arcana_core::definition_hash(&d.definition));
        store.upsert_semantic_definition(&d).await?;
        count += 1;
    }
    texts.clear();
    Ok(count)
}

/// Check whether any definition in the list comes from an authoritative source
/// (manual, dbt YAML, or Snowflake comment).
fn has_authoritative_definition(defs: &[SemanticDefinition]) -> bool {
    defs.iter().any(|d| {
        matches!(
            d.source,
            DefinitionSource::Manual | DefinitionSource::DbtYaml | DefinitionSource::SnowflakeComment
        )
    })
}
