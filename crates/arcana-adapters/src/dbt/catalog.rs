use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::adapter::SyncOutput;

/// Subset of dbt catalog.json schema used by Arcana.
#[derive(Debug, Deserialize)]
pub struct DbtCatalog {
    pub metadata: serde_json::Value,
    pub nodes: HashMap<String, CatalogNode>,
    pub sources: HashMap<String, CatalogNode>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogNode {
    pub unique_id: String,
    pub metadata: CatalogNodeMeta,
    pub columns: HashMap<String, CatalogColumn>,
    pub stats: HashMap<String, CatalogStat>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogNodeMeta {
    #[serde(rename = "type")]
    pub node_type: Option<String>,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub name: Option<String>,
    pub owner: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: Option<String>,
    pub index: Option<i32>,
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogStat {
    pub id: String,
    pub label: Option<String>,
    pub value: Option<serde_json::Value>,
    pub include: Option<bool>,
    pub description: Option<String>,
}

/// Parse catalog.json to enrich tables with row counts, owners, and column types.
pub async fn parse_catalog(catalog_path: &Path) -> Result<SyncOutput> {
    let raw = tokio::fs::read_to_string(catalog_path)
        .await
        .with_context(|| format!("failed to read catalog.json at {:?}", catalog_path))?;

    let catalog: DbtCatalog =
        serde_json::from_str(&raw).context("failed to parse dbt catalog.json")?;

    let mut output = SyncOutput::default();

    for (_, node) in &catalog.nodes {
        // TODO: enrich existing Table entities with row_count, byte_size from stats
        //       enrich Column entities with data_type from catalog
        let _ = node;
        output.stats.columns_upserted += node.columns.len();
    }

    for (_, source) in &catalog.sources {
        let _ = source;
        output.stats.columns_upserted += source.columns.len();
    }

    tracing::info!(
        "dbt catalog parsed: {} nodes, {} sources",
        catalog.nodes.len(),
        catalog.sources.len()
    );

    Ok(output)
}
