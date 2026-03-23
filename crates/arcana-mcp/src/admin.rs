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
use sha2::{Digest, Sha256};
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tower_http::limit::RequestBodyLimitLayer;

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
// Known adapters for webhook validation
// ---------------------------------------------------------------------------

const KNOWN_ADAPTERS: &[&str] = &["dbt", "snowflake"];

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
        .layer(RequestBodyLimitLayer::new(512 * 1024)) // 512KB body limit
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
// Timing-safe token comparison
// ---------------------------------------------------------------------------

/// Compare two strings in constant time by hashing both with SHA-256 first.
/// This normalizes length and prevents timing side-channels.
fn verify_token(provided: &str, expected: &str) -> bool {
    let provided_hash = Sha256::digest(provided.as_bytes());
    let expected_hash = Sha256::digest(expected.as_bytes());
    provided_hash.ct_eq(&expected_hash).into()
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
    // Validate webhook secret if configured (timing-safe)
    if let Some(secret) = &state.webhook_secret {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let expected = format!("Bearer {secret}");
        if !verify_token(auth, &expected) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    // Validate adapter parameter against known values
    if let Some(adapter) = &body.adapter {
        if !KNOWN_ADAPTERS.contains(&adapter.as_str()) {
            return Ok(Json(SyncWebhookResponse {
                accepted: false,
                message: format!(
                    "Unknown adapter '{}'. Known adapters: {}",
                    adapter,
                    KNOWN_ADAPTERS.join(", ")
                ),
            }));
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
        let adapter_msg = body.adapter.as_deref().unwrap_or("all");
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

    let coverage = store
        .count_tables_and_coverage()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let metrics = store
        .list_metrics()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all_defs = store
        .list_all_semantic_definitions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let defs_with_embeddings = all_defs.iter().filter(|d| d.embedding.is_some()).count();
    let clusters = store
        .list_table_clusters()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let coverage_pct = if coverage.total_tables > 0 {
        (coverage.tables_with_definitions as f64 / coverage.total_tables as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(StatsResponse {
        data_sources: coverage.data_source_count,
        tables: coverage.total_tables,
        columns: coverage.total_columns,
        metrics: metrics.len(),
        definitions: all_defs.len(),
        definitions_with_embeddings: defs_with_embeddings,
        index_vectors: state.entity_index.len(),
        definition_coverage_pct: (coverage_pct * 100.0).round() / 100.0,
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
    let coverage = state
        .store
        .count_tables_and_coverage()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let schemas = coverage
        .schema_coverages
        .into_iter()
        .map(|sc| {
            let pct = if sc.total_tables > 0 {
                (sc.tables_with_definitions as f64 / sc.total_tables as f64) * 100.0
            } else {
                0.0
            };
            SchemaCoverage {
                data_source: sc.data_source_name,
                schema_name: format!("{}.{}", sc.database_name, sc.schema_name),
                total_tables: sc.total_tables,
                tables_with_definitions: sc.tables_with_definitions,
                coverage_pct: (pct * 100.0).round() / 100.0,
            }
        })
        .collect();

    Ok(Json(CoverageResponse { schemas }))
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
    let records = state
        .store
        .list_recent_evidence(100)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let entries = records
        .into_iter()
        .map(|r| EvidenceEntry {
            entity_id: r.entity_id.to_string(),
            outcome: format!("{:?}", r.outcome),
            confidence_delta: r.confidence_delta,
            query_text: r.query_text,
            created_at: r.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(EvidenceResponse { records: entries }))
}
