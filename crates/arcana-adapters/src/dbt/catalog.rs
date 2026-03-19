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
    #[serde(default)]
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

/// Enrich an existing SyncOutput (from manifest parsing) with catalog.json data.
///
/// Matches tables by name (lowercased) and enriches with:
/// - row_count and byte_size from catalog stats
/// - owner from catalog metadata
/// - data_type for columns from catalog column types
pub async fn enrich_from_catalog(catalog_path: &Path, output: &mut SyncOutput) -> Result<()> {
    let raw = tokio::fs::read_to_string(catalog_path)
        .await
        .with_context(|| format!("failed to read catalog.json at {:?}", catalog_path))?;

    let catalog: DbtCatalog =
        serde_json::from_str(&raw).context("failed to parse dbt catalog.json")?;

    // Build a lookup of table name (lowercased) → index in output.tables
    let mut table_index_by_name: HashMap<String, usize> = HashMap::new();
    for (i, table) in output.tables.iter().enumerate() {
        table_index_by_name.insert(table.name.to_lowercase(), i);
    }

    // Build a lookup of (table_id, column_name_lower) → index in output.columns
    let mut col_index_by_key: HashMap<(uuid::Uuid, String), usize> = HashMap::new();
    for (i, col) in output.columns.iter().enumerate() {
        col_index_by_key.insert((col.table_id, col.name.to_lowercase()), i);
    }

    let all_catalog_nodes = catalog.nodes.values().chain(catalog.sources.values());

    for catalog_node in all_catalog_nodes {
        let node_name = catalog_node
            .metadata
            .name
            .as_deref()
            .unwrap_or("")
            .to_lowercase();

        let Some(&table_idx) = table_index_by_name.get(&node_name) else {
            continue;
        };

        let table = &mut output.tables[table_idx];

        // Enrich table with owner
        if table.owner.is_none() {
            table.owner.clone_from(&catalog_node.metadata.owner);
        }

        // Enrich table with stats
        if let Some(row_count) = extract_stat_i64(&catalog_node.stats, "row_count") {
            table.row_count = Some(row_count);
        }
        if let Some(bytes) = extract_stat_i64(&catalog_node.stats, "bytes") {
            table.byte_size = Some(bytes);
        }

        // Enrich columns with data_type from catalog
        let table_id = table.id;
        for (_, catalog_col) in &catalog_node.columns {
            let col_key = (table_id, catalog_col.name.to_lowercase());
            if let Some(&col_idx) = col_index_by_key.get(&col_key) {
                if let Some(data_type) = &catalog_col.data_type {
                    output.columns[col_idx].data_type = data_type.clone();
                }
                if let Some(index) = catalog_col.index {
                    output.columns[col_idx].ordinal_position = index;
                }
            }
        }
    }

    tracing::info!(
        "dbt catalog enrichment complete: {} nodes, {} sources",
        catalog.nodes.len(),
        catalog.sources.len(),
    );

    Ok(())
}

/// Extract an i64 stat value from a catalog stats map.
fn extract_stat_i64(stats: &HashMap<String, CatalogStat>, key: &str) -> Option<i64> {
    let stat = stats.get(key)?;
    let value = stat.value.as_ref()?;
    // dbt catalog stats can be numbers or numeric strings
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::SyncOutput;
    use arcana_core::entities::{Column, Schema, Table, TableType};
    use chrono::Utc;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use uuid::Uuid;

    fn make_test_output() -> SyncOutput {
        let now = Utc::now();
        let schema_id = Uuid::new_v4();
        let table_id = Uuid::new_v4();

        let mut output = SyncOutput::default();
        output.schemas.push(Schema {
            id: schema_id,
            data_source_id: Uuid::new_v4(),
            database_name: "prod".to_string(),
            schema_name: "analytics".to_string(),
            created_at: now,
            updated_at: now,
        });
        output.tables.push(Table {
            id: table_id,
            schema_id,
            name: "orders".to_string(),
            table_type: TableType::BaseTable,
            description: Some("Orders table".to_string()),
            dbt_model: Some("orders".to_string()),
            owner: None,
            row_count: None,
            byte_size: None,
            confidence: 0.80,
            confidence_refreshed_at: Some(now),
            tags: serde_json::json!([]),
            created_at: now,
            updated_at: now,
        });
        output.columns.push(Column {
            id: Uuid::new_v4(),
            table_id,
            name: "order_id".to_string(),
            data_type: "unknown".to_string(),
            ordinal_position: 0,
            is_nullable: true,
            is_primary_key: false,
            is_foreign_key: false,
            description: Some("Unique order ID".to_string()),
            dbt_meta: None,
            tags: serde_json::json!([]),
            confidence: 0.80,
            confidence_refreshed_at: Some(now),
            created_at: now,
            updated_at: now,
        });
        output
    }

    fn sample_catalog_json() -> serde_json::Value {
        serde_json::json!({
            "metadata": {},
            "nodes": {
                "model.my_project.orders": {
                    "unique_id": "model.my_project.orders",
                    "metadata": {
                        "type": "BASE TABLE",
                        "schema": "ANALYTICS",
                        "database": "PROD",
                        "name": "orders",
                        "owner": "analytics_team",
                        "comment": null
                    },
                    "columns": {
                        "ORDER_ID": {
                            "name": "order_id",
                            "type": "NUMBER(38,0)",
                            "index": 1,
                            "comment": null
                        }
                    },
                    "stats": {
                        "row_count": {
                            "id": "row_count",
                            "label": "Row Count",
                            "value": 150000,
                            "include": true,
                            "description": "Number of rows"
                        },
                        "bytes": {
                            "id": "bytes",
                            "label": "Approximate Size",
                            "value": "524288",
                            "include": true,
                            "description": "Size in bytes"
                        }
                    }
                }
            },
            "sources": {}
        })
    }

    #[tokio::test]
    async fn test_enrich_table_stats() {
        let mut output = make_test_output();
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", sample_catalog_json()).unwrap();

        enrich_from_catalog(tmp.path(), &mut output).await.unwrap();

        let table = &output.tables[0];
        assert_eq!(table.owner, Some("analytics_team".to_string()));
        assert_eq!(table.row_count, Some(150000));
        assert_eq!(table.byte_size, Some(524288)); // parsed from string
    }

    #[tokio::test]
    async fn test_enrich_column_types() {
        let mut output = make_test_output();
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", sample_catalog_json()).unwrap();

        enrich_from_catalog(tmp.path(), &mut output).await.unwrap();

        let col = &output.columns[0];
        assert_eq!(col.data_type, "NUMBER(38,0)");
        assert_eq!(col.ordinal_position, 1);
    }
}
