use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::adapter::SyncOutput;

/// Subset of dbt manifest.json schema used by Arcana.
/// Full spec: https://schemas.getdbt.com/dbt/manifest/
#[derive(Debug, Deserialize)]
pub struct DbtManifest {
    pub metadata: ManifestMetadata,
    pub nodes: HashMap<String, DbtNode>,
    pub sources: HashMap<String, DbtSource>,
    pub exposures: HashMap<String, serde_json::Value>,
    pub metrics: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestMetadata {
    pub dbt_schema_version: Option<String>,
    pub dbt_version: Option<String>,
    pub generated_at: Option<String>,
    pub adapter_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DbtNode {
    pub unique_id: String,
    pub name: String,
    pub resource_type: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub description: Option<String>,
    pub columns: HashMap<String, DbtColumnDef>,
    pub depends_on: Option<DbtDependsOn>,
    pub tags: Vec<String>,
    pub config: Option<serde_json::Value>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct DbtSource {
    pub unique_id: String,
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub description: Option<String>,
    pub columns: HashMap<String, DbtColumnDef>,
    pub tags: Vec<String>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DbtColumnDef {
    pub name: String,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub meta: Option<serde_json::Value>,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct DbtDependsOn {
    pub nodes: Vec<String>,
}

/// Parse manifest.json and extract tables, columns, and lineage edges.
pub async fn parse_manifest(manifest_path: &Path) -> Result<SyncOutput> {
    let raw = tokio::fs::read_to_string(manifest_path)
        .await
        .with_context(|| format!("failed to read manifest.json at {:?}", manifest_path))?;

    let manifest: DbtManifest =
        serde_json::from_str(&raw).context("failed to parse dbt manifest.json")?;

    let mut output = SyncOutput::default();

    // Map dbt nodes (models) → tables and columns
    for (_, node) in &manifest.nodes {
        if node.resource_type != "model" && node.resource_type != "snapshot" {
            continue;
        }
        // TODO: map DbtNode → arcana_core::entities::Table
        //       and DbtColumnDef → arcana_core::entities::Column
        //       and depends_on.nodes → arcana_core::entities::LineageEdge
        let _ = node;
        output.stats.tables_upserted += 1;
    }

    // Map dbt sources → tables
    for (_, source) in &manifest.sources {
        let _ = source;
        output.stats.tables_upserted += 1;
    }

    tracing::info!(
        "dbt manifest parsed: {} models, {} sources",
        manifest.nodes.len(),
        manifest.sources.len()
    );

    Ok(output)
}
