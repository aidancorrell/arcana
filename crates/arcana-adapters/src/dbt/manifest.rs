use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use arcana_core::entities::{
    Column, DefinitionSource, LineageEdge, LineageNodeType, LineageSource, Schema,
    SemanticDefinition, SemanticEntityType, Table, TableType,
};

use crate::adapter::SyncOutput;

/// Subset of dbt manifest.json schema used by Arcana.
/// Full spec: https://schemas.getdbt.com/dbt/manifest/
#[derive(Debug, Deserialize)]
pub struct DbtManifest {
    pub metadata: ManifestMetadata,
    pub nodes: HashMap<String, DbtNode>,
    pub sources: HashMap<String, DbtSource>,
    #[serde(default)]
    pub exposures: HashMap<String, serde_json::Value>,
    #[serde(default)]
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
    #[serde(default)]
    pub columns: HashMap<String, DbtColumnDef>,
    pub depends_on: Option<DbtDependsOn>,
    #[serde(default)]
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
    #[serde(default)]
    pub columns: HashMap<String, DbtColumnDef>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DbtColumnDef {
    pub name: String,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub meta: Option<serde_json::Value>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct DbtDependsOn {
    #[serde(default)]
    pub nodes: Vec<String>,
}

/// Parse manifest.json and extract tables, columns, semantic definitions, and lineage edges.
pub async fn parse_manifest(manifest_path: &Path, data_source_id: Uuid) -> Result<SyncOutput> {
    let raw = tokio::fs::read_to_string(manifest_path)
        .await
        .with_context(|| format!("failed to read manifest.json at {:?}", manifest_path))?;

    let manifest: DbtManifest =
        serde_json::from_str(&raw).context("failed to parse dbt manifest.json")?;

    let mut output = SyncOutput::default();

    // Track schemas by (database, schema_name) to deduplicate
    let mut schema_map: HashMap<(String, String), Uuid> = HashMap::new();

    // Track table IDs by unique_id so we can resolve lineage references
    let mut table_id_by_unique_id: HashMap<String, Uuid> = HashMap::new();

    // Process nodes (models and snapshots)
    for (_, node) in &manifest.nodes {
        if node.resource_type != "model" && node.resource_type != "snapshot" {
            continue;
        }

        let db = node.database.clone().unwrap_or_else(|| "default".to_string());
        let schema_name = node.schema.clone().unwrap_or_else(|| "public".to_string());

        let schema_id = get_or_create_schema(
            &mut schema_map,
            &mut output,
            data_source_id,
            &db,
            &schema_name,
        );

        let table_type = if node.resource_type == "snapshot" {
            TableType::BaseTable
        } else {
            // dbt models are typically materialized as views or tables
            match node.config.as_ref().and_then(|c| c.get("materialized")).and_then(|v| v.as_str())
            {
                Some("view") => TableType::View,
                Some("materialized_view") => TableType::MaterializedView,
                _ => TableType::BaseTable,
            }
        };

        let now = Utc::now();
        let table_id = Uuid::new_v4();
        table_id_by_unique_id.insert(node.unique_id.clone(), table_id);

        let table = Table {
            id: table_id,
            schema_id,
            name: node.name.clone(),
            table_type,
            description: node.description.clone(),
            dbt_model: Some(node.name.clone()),
            owner: None,
            row_count: None,
            byte_size: None,
            confidence: 0.80,
            confidence_refreshed_at: Some(now),
            tags: serde_json::json!(node.tags),
            created_at: now,
            updated_at: now,
        };
        output.tables.push(table);
        output.stats.tables_upserted += 1;

        // Create SemanticDefinition for table description
        if let Some(desc) = &node.description {
            if !desc.is_empty() {
                output.semantic_definitions.push(SemanticDefinition {
                    id: Uuid::new_v4(),
                    entity_id: table_id,
                    entity_type: SemanticEntityType::Table,
                    definition: desc.clone(),
                    source: DefinitionSource::DbtYaml,
                    confidence: 0.80,
                    confidence_refreshed_at: Some(now),
                    embedding: None,
                    created_at: now,
                    updated_at: now,
                });
            }
        }

        // Process columns
        process_columns(&node.columns, table_id, &mut output);

    }

    // Process sources (external tables)
    for (_, source) in &manifest.sources {
        let db = source
            .database
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let schema_name = source
            .schema
            .clone()
            .unwrap_or_else(|| "public".to_string());

        let schema_id = get_or_create_schema(
            &mut schema_map,
            &mut output,
            data_source_id,
            &db,
            &schema_name,
        );

        let now = Utc::now();
        let table_id = Uuid::new_v4();
        table_id_by_unique_id.insert(source.unique_id.clone(), table_id);

        let table = Table {
            id: table_id,
            schema_id,
            name: source.name.clone(),
            table_type: TableType::ExternalTable,
            description: source.description.clone(),
            dbt_model: None,
            owner: None,
            row_count: None,
            byte_size: None,
            confidence: 0.80,
            confidence_refreshed_at: Some(now),
            tags: serde_json::json!(source.tags),
            created_at: now,
            updated_at: now,
        };
        output.tables.push(table);
        output.stats.tables_upserted += 1;

        // Create SemanticDefinition for source description
        if let Some(desc) = &source.description {
            if !desc.is_empty() {
                output.semantic_definitions.push(SemanticDefinition {
                    id: Uuid::new_v4(),
                    entity_id: table_id,
                    entity_type: SemanticEntityType::Table,
                    definition: desc.clone(),
                    source: DefinitionSource::DbtYaml,
                    confidence: 0.80,
                    confidence_refreshed_at: Some(now),
                    embedding: None,
                    created_at: now,
                    updated_at: now,
                });
            }
        }

        // Process columns
        process_columns(&source.columns, table_id, &mut output);
    }

    // Resolve lineage edges now that all table IDs are known
    for (_, node) in &manifest.nodes {
        if node.resource_type != "model" && node.resource_type != "snapshot" {
            continue;
        }
        let Some(&downstream_id) = table_id_by_unique_id.get(&node.unique_id) else {
            continue;
        };
        if let Some(depends_on) = &node.depends_on {
            for upstream_unique_id in &depends_on.nodes {
                if let Some(&upstream_id) = table_id_by_unique_id.get(upstream_unique_id) {
                    let now = Utc::now();
                    output.lineage_edges.push(LineageEdge {
                        id: Uuid::new_v4(),
                        upstream_id,
                        upstream_type: LineageNodeType::Table,
                        downstream_id,
                        downstream_type: LineageNodeType::Table,
                        source: LineageSource::DbtManifest,
                        transform_expression: None,
                        confidence: 1.0,
                        created_at: now,
                        updated_at: now,
                    });
                    output.stats.lineage_edges_upserted += 1;
                }
            }
        }
    }

    tracing::info!(
        "dbt manifest parsed: {} tables, {} columns, {} definitions, {} lineage edges",
        output.stats.tables_upserted,
        output.stats.columns_upserted,
        output.semantic_definitions.len(),
        output.stats.lineage_edges_upserted,
    );

    Ok(output)
}

/// Get or create a Schema for the given (database, schema_name) pair.
fn get_or_create_schema(
    schema_map: &mut HashMap<(String, String), Uuid>,
    output: &mut SyncOutput,
    data_source_id: Uuid,
    database: &str,
    schema_name: &str,
) -> Uuid {
    let key = (database.to_string(), schema_name.to_string());
    if let Some(&id) = schema_map.get(&key) {
        return id;
    }

    let now = Utc::now();
    let schema_id = Uuid::new_v4();
    schema_map.insert(key, schema_id);

    output.schemas.push(Schema {
        id: schema_id,
        data_source_id,
        database_name: database.to_string(),
        schema_name: schema_name.to_string(),
        created_at: now,
        updated_at: now,
    });
    output.stats.schemas_upserted += 1;

    schema_id
}

/// Process dbt column definitions into Column entities and SemanticDefinitions.
fn process_columns(
    columns: &HashMap<String, DbtColumnDef>,
    table_id: Uuid,
    output: &mut SyncOutput,
) {
    let now = Utc::now();
    for (i, (_, col_def)) in columns.iter().enumerate() {
        let col_id = Uuid::new_v4();

        output.columns.push(Column {
            id: col_id,
            table_id,
            name: col_def.name.clone(),
            data_type: col_def.data_type.clone().unwrap_or_else(|| "unknown".to_string()),
            ordinal_position: i as i32,
            is_nullable: true,  // dbt doesn't expose this; default to true
            is_primary_key: false,
            is_foreign_key: false,
            description: col_def.description.clone(),
            dbt_meta: col_def.meta.clone(),
            tags: serde_json::json!(col_def.tags),
            confidence: 0.80,
            confidence_refreshed_at: Some(now),
            created_at: now,
            updated_at: now,
        });
        output.stats.columns_upserted += 1;

        // Create SemanticDefinition for column description
        if let Some(desc) = &col_def.description {
            if !desc.is_empty() {
                output.semantic_definitions.push(SemanticDefinition {
                    id: Uuid::new_v4(),
                    entity_id: col_id,
                    entity_type: SemanticEntityType::Column,
                    definition: desc.clone(),
                    source: DefinitionSource::DbtYaml,
                    confidence: 0.80,
                    confidence_refreshed_at: Some(now),
                    embedding: None,
                    created_at: now,
                    updated_at: now,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn sample_manifest_json() -> serde_json::Value {
        serde_json::json!({
            "metadata": {
                "dbt_schema_version": "https://schemas.getdbt.com/dbt/manifest/v12.json",
                "dbt_version": "1.7.0",
                "generated_at": "2024-01-01T00:00:00Z",
                "adapter_type": "snowflake"
            },
            "nodes": {
                "model.my_project.orders": {
                    "unique_id": "model.my_project.orders",
                    "name": "orders",
                    "resource_type": "model",
                    "schema": "analytics",
                    "database": "prod",
                    "description": "Core orders table with all completed transactions",
                    "columns": {
                        "order_id": {
                            "name": "order_id",
                            "description": "Unique order identifier",
                            "data_type": "INT64",
                            "tags": [],
                            "meta": {}
                        },
                        "customer_id": {
                            "name": "customer_id",
                            "description": "FK to customers table",
                            "data_type": "INT64",
                            "tags": [],
                            "meta": {}
                        }
                    },
                    "depends_on": {
                        "nodes": ["source.my_project.raw.orders_raw"]
                    },
                    "tags": ["core", "finance"],
                    "config": {"materialized": "table"},
                    "meta": {}
                },
                "test.my_project.not_null_orders": {
                    "unique_id": "test.my_project.not_null_orders",
                    "name": "not_null_orders",
                    "resource_type": "test",
                    "schema": "analytics",
                    "database": "prod",
                    "description": "",
                    "columns": {},
                    "depends_on": {"nodes": []},
                    "tags": [],
                    "config": {},
                    "meta": {}
                }
            },
            "sources": {
                "source.my_project.raw.orders_raw": {
                    "unique_id": "source.my_project.raw.orders_raw",
                    "name": "orders_raw",
                    "schema": "raw",
                    "database": "prod",
                    "description": "Raw orders from the transactional system",
                    "columns": {
                        "id": {
                            "name": "id",
                            "description": "",
                            "data_type": null,
                            "tags": [],
                            "meta": {}
                        }
                    },
                    "tags": ["raw"],
                    "meta": {}
                }
            },
            "exposures": {},
            "metrics": {}
        })
    }

    #[tokio::test]
    async fn test_parse_manifest_tables() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", sample_manifest_json()).unwrap();

        let ds_id = Uuid::new_v4();
        let output = parse_manifest(tmp.path(), ds_id).await.unwrap();

        // Should have 2 tables: orders (model) + orders_raw (source), NOT the test node
        assert_eq!(output.tables.len(), 2);
        assert_eq!(output.stats.tables_upserted, 2);

        let model = output.tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(model.table_type, TableType::BaseTable);
        assert_eq!(model.dbt_model, Some("orders".to_string()));
        assert_eq!(model.confidence, 0.80);

        let source = output.tables.iter().find(|t| t.name == "orders_raw").unwrap();
        assert_eq!(source.table_type, TableType::ExternalTable);
        assert!(source.dbt_model.is_none());
    }

    #[tokio::test]
    async fn test_parse_manifest_columns() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", sample_manifest_json()).unwrap();

        let output = parse_manifest(tmp.path(), Uuid::new_v4()).await.unwrap();

        // orders has 2 columns, orders_raw has 1
        assert_eq!(output.columns.len(), 3);

        let order_id_col = output.columns.iter().find(|c| c.name == "order_id").unwrap();
        assert_eq!(order_id_col.data_type, "INT64");
        assert_eq!(order_id_col.confidence, 0.80);
    }

    #[tokio::test]
    async fn test_parse_manifest_semantic_definitions() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", sample_manifest_json()).unwrap();

        let output = parse_manifest(tmp.path(), Uuid::new_v4()).await.unwrap();

        // Table descriptions: "orders" + "orders_raw" = 2
        // Column descriptions: "order_id" + "customer_id" = 2 (source column "id" has empty desc)
        assert_eq!(output.semantic_definitions.len(), 4);

        let table_defs: Vec<_> = output
            .semantic_definitions
            .iter()
            .filter(|d| d.entity_type == SemanticEntityType::Table)
            .collect();
        assert_eq!(table_defs.len(), 2);
        assert!(table_defs.iter().all(|d| d.source == DefinitionSource::DbtYaml));
        assert!(table_defs.iter().all(|d| d.confidence == 0.80));
    }

    #[tokio::test]
    async fn test_parse_manifest_schemas_dedup() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", sample_manifest_json()).unwrap();

        let ds_id = Uuid::new_v4();
        let output = parse_manifest(tmp.path(), ds_id).await.unwrap();

        // Two distinct schemas: (prod, analytics) and (prod, raw)
        assert_eq!(output.schemas.len(), 2);
        assert!(output.schemas.iter().all(|s| s.data_source_id == ds_id));
    }

    #[tokio::test]
    async fn test_parse_manifest_lineage() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", sample_manifest_json()).unwrap();

        let output = parse_manifest(tmp.path(), Uuid::new_v4()).await.unwrap();

        // orders depends_on orders_raw → 1 lineage edge
        assert_eq!(output.lineage_edges.len(), 1);

        let edge = &output.lineage_edges[0];
        assert_eq!(edge.source, LineageSource::DbtManifest);
        assert_eq!(edge.confidence, 1.0);
        assert_eq!(edge.upstream_type, LineageNodeType::Table);
        assert_eq!(edge.downstream_type, LineageNodeType::Table);

        // Verify the edge points from source → model
        let source_table = output.tables.iter().find(|t| t.name == "orders_raw").unwrap();
        let model_table = output.tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(edge.upstream_id, source_table.id);
        assert_eq!(edge.downstream_id, model_table.id);
    }

    #[tokio::test]
    async fn test_parse_manifest_view_materialization() {
        let manifest = serde_json::json!({
            "metadata": {"dbt_schema_version": null, "dbt_version": null, "generated_at": null, "adapter_type": null},
            "nodes": {
                "model.proj.my_view": {
                    "unique_id": "model.proj.my_view",
                    "name": "my_view",
                    "resource_type": "model",
                    "schema": "public",
                    "database": "db",
                    "description": "",
                    "columns": {},
                    "depends_on": {"nodes": []},
                    "tags": [],
                    "config": {"materialized": "view"},
                    "meta": {}
                }
            },
            "sources": {},
            "exposures": {},
            "metrics": {}
        });

        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", manifest).unwrap();

        let output = parse_manifest(tmp.path(), Uuid::new_v4()).await.unwrap();
        assert_eq!(output.tables[0].table_type, TableType::View);
    }
}
