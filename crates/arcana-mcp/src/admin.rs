//! Admin API and webhook endpoints for Arcana.
//!
//! Runs on a separate port from the MCP SSE server. Provides:
//! - `POST /api/sync`          — webhook to trigger metadata sync
//! - `GET  /api/admin/stats`   — entity counts, definition coverage, index size
//! - `GET  /api/admin/coverage`— per-schema definition coverage breakdown
//! - `GET  /api/admin/evidence`— recent evidence records (feedback trail)
//! - `GET  /health`            — basic health check

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use arcana_core::embeddings::VectorIndex;
use arcana_core::store::MetadataStore;

// ---------------------------------------------------------------------------
// Shared state for the admin server
// ---------------------------------------------------------------------------

/// Everything the admin API needs to answer requests.
#[derive(Clone)]
pub struct AdminState {
    pub store: Arc<dyn MetadataStore>,
    pub entity_index: Arc<VectorIndex>,
    /// If set, `POST /api/sync` requires `Authorization: Bearer <key>`.
    pub webhook_secret: Option<String>,
    /// Channel to signal the sync worker to run now.
    pub sync_trigger: Option<tokio::sync::mpsc::Sender<()>>,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn admin_router(state: AdminState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/sync", post(sync_webhook_handler))
        .route("/api/admin/stats", get(stats_handler))
        .route("/api/admin/coverage", get(coverage_handler))
        .route("/api/admin/evidence", get(evidence_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

// ---------------------------------------------------------------------------
// POST /api/sync — webhook to trigger sync
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SyncWebhookBody {
    /// Optional adapter filter (e.g. "dbt", "snowflake"). If absent, syncs all.
    #[serde(default)]
    adapter: Option<String>,
}

#[derive(Serialize)]
struct SyncWebhookResponse {
    accepted: bool,
    message: String,
}

async fn sync_webhook_handler(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<SyncWebhookBody>,
) -> Result<Json<SyncWebhookResponse>, StatusCode> {
    // Validate webhook secret if configured
    if let Some(secret) = &state.webhook_secret {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let expected = format!("Bearer {secret}");
        if auth != expected {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    // Signal the sync worker
    if let Some(trigger) = &state.sync_trigger {
        if trigger.try_send(()).is_err() {
            return Ok(Json(SyncWebhookResponse {
                accepted: false,
                message: "Sync already in progress.".to_string(),
            }));
        }
        let adapter_msg = body
            .adapter
            .as_deref()
            .unwrap_or("all");
        Ok(Json(SyncWebhookResponse {
            accepted: true,
            message: format!("Sync triggered for adapter: {adapter_msg}"),
        }))
    } else {
        Ok(Json(SyncWebhookResponse {
            accepted: false,
            message: "Sync is not configured on this server.".to_string(),
        }))
    }
}

// ---------------------------------------------------------------------------
// GET /api/admin/stats
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct StatsResponse {
    data_sources: usize,
    tables: usize,
    columns: usize,
    metrics: usize,
    definitions: usize,
    definitions_with_embeddings: usize,
    index_vectors: usize,
    definition_coverage_pct: f64,
    clusters: usize,
}

async fn stats_handler(
    State(state): State<AdminState>,
) -> Result<Json<StatsResponse>, StatusCode> {
    let store = &state.store;

    let data_sources = store.list_data_sources().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut table_count = 0usize;
    let mut column_count = 0usize;
    let mut tables_with_defs = 0usize;

    for ds in &data_sources {
        let schemas = store.list_schemas(ds.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for schema in &schemas {
            let tables = store.list_tables(schema.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            for table in &tables {
                table_count += 1;
                let defs = store.get_semantic_definitions(table.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                if !defs.is_empty() {
                    tables_with_defs += 1;
                }
                let cols = store.list_columns(table.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                column_count += cols.len();
            }
        }
    }

    let metrics = store.list_metrics().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all_defs = store.list_all_semantic_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let defs_with_embeddings = all_defs.iter().filter(|d| d.embedding.is_some()).count();
    let clusters = store.list_table_clusters().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let coverage = if table_count > 0 {
        (tables_with_defs as f64 / table_count as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(StatsResponse {
        data_sources: data_sources.len(),
        tables: table_count,
        columns: column_count,
        metrics: metrics.len(),
        definitions: all_defs.len(),
        definitions_with_embeddings: defs_with_embeddings,
        index_vectors: state.entity_index.len(),
        definition_coverage_pct: (coverage * 100.0).round() / 100.0,
        clusters: clusters.len(),
    }))
}

// ---------------------------------------------------------------------------
// GET /api/admin/coverage — per-schema breakdown
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CoverageResponse {
    schemas: Vec<SchemaCoverage>,
}

#[derive(Serialize)]
struct SchemaCoverage {
    data_source: String,
    schema_name: String,
    total_tables: usize,
    tables_with_definitions: usize,
    coverage_pct: f64,
}

async fn coverage_handler(
    State(state): State<AdminState>,
) -> Result<Json<CoverageResponse>, StatusCode> {
    let store = &state.store;
    let data_sources = store.list_data_sources().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut schema_coverages = Vec::new();

    for ds in &data_sources {
        let schemas = store.list_schemas(ds.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for schema in &schemas {
            let tables = store.list_tables(schema.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let total = tables.len();
            let mut with_defs = 0usize;
            for table in &tables {
                let defs = store.get_semantic_definitions(table.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                if !defs.is_empty() {
                    with_defs += 1;
                }
            }
            let coverage = if total > 0 {
                (with_defs as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            schema_coverages.push(SchemaCoverage {
                data_source: ds.name.clone(),
                schema_name: format!("{}.{}", schema.database_name, schema.schema_name),
                total_tables: total,
                tables_with_definitions: with_defs,
                coverage_pct: (coverage * 100.0).round() / 100.0,
            });
        }
    }

    Ok(Json(CoverageResponse { schemas: schema_coverages }))
}

// ---------------------------------------------------------------------------
// GET /api/admin/evidence — recent evidence records
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct EvidenceResponse {
    records: Vec<EvidenceEntry>,
}

#[derive(Serialize)]
struct EvidenceEntry {
    entity_id: String,
    outcome: String,
    confidence_delta: f64,
    query_text: Option<String>,
    created_at: String,
}

async fn evidence_handler(
    State(state): State<AdminState>,
) -> Result<Json<EvidenceResponse>, StatusCode> {
    let store = &state.store;

    // Collect evidence from all known entities (tables)
    // In practice this would be a dedicated query; for now, iterate
    let data_sources = store.list_data_sources().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut all_evidence = Vec::new();

    for ds in &data_sources {
        let schemas = store.list_schemas(ds.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for schema in &schemas {
            let tables = store.list_tables(schema.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            for table in &tables {
                let evidence = store
                    .get_evidence_for_entity(table.id)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                for record in evidence {
                    all_evidence.push(EvidenceEntry {
                        entity_id: record.entity_id.to_string(),
                        outcome: format!("{:?}", record.outcome),
                        confidence_delta: record.confidence_delta,
                        query_text: record.query_text,
                        created_at: record.created_at.to_rfc3339(),
                    });
                }
            }
        }
    }

    // Sort by created_at descending, limit to 100
    all_evidence.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_evidence.truncate(100);

    Ok(Json(EvidenceResponse { records: all_evidence }))
}
