use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

use arcana_core::entities::{
    Column, DefinitionSource, Schema, SemanticDefinition, SemanticEntityType, Table, TableType,
};

use super::client::{column_index, get_cell, SnowflakeClient};
use super::SnowflakeConfig;
use crate::adapter::SyncOutput;

/// Sync schemas, tables, and columns from Snowflake INFORMATION_SCHEMA.
pub async fn sync_schemas(config: &SnowflakeConfig, data_source_id: Uuid) -> Result<SyncOutput> {
    let mut client = SnowflakeClient::new(config.clone());
    let mut output = SyncOutput::default();

    // Track schemas by name → id for FK linking
    let mut schema_map: HashMap<String, Uuid> = HashMap::new();
    // Track tables by (schema_name, table_name) → id
    let mut table_map: HashMap<(String, String), Uuid> = HashMap::new();

    // 1. Sync schemas
    let schemas_resp = client
        .execute_sql(&format!(
            "SELECT SCHEMA_NAME, CATALOG_NAME \
             FROM {}.INFORMATION_SCHEMA.SCHEMATA \
             WHERE SCHEMA_NAME NOT IN ('INFORMATION_SCHEMA') \
             ORDER BY SCHEMA_NAME",
            config.database
        ))
        .await
        .context("failed to query INFORMATION_SCHEMA.SCHEMATA")?;

    let schema_name_idx = column_index(&schemas_resp.result_set_metadata, "SCHEMA_NAME")
        .unwrap_or(0);
    let catalog_name_idx = column_index(&schemas_resp.result_set_metadata, "CATALOG_NAME")
        .unwrap_or(1);

    for row in &schemas_resp.data {
        let schema_name = get_cell(row, schema_name_idx).unwrap_or("").to_string();
        let db_name = get_cell(row, catalog_name_idx)
            .unwrap_or(&config.database)
            .to_string();

        let now = Utc::now();
        let schema_id = Uuid::new_v4();
        schema_map.insert(schema_name.clone(), schema_id);

        output.schemas.push(Schema {
            id: schema_id,
            data_source_id,
            database_name: db_name,
            schema_name,
            created_at: now,
            updated_at: now,
        });
        output.stats.schemas_upserted += 1;
    }

    // 2. Sync tables
    let tables_resp = client
        .execute_sql(&format!(
            "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, COMMENT, ROW_COUNT, BYTES \
             FROM {}.INFORMATION_SCHEMA.TABLES \
             WHERE TABLE_SCHEMA NOT IN ('INFORMATION_SCHEMA') \
             ORDER BY TABLE_SCHEMA, TABLE_NAME",
            config.database
        ))
        .await
        .context("failed to query INFORMATION_SCHEMA.TABLES")?;

    let t_schema_idx =
        column_index(&tables_resp.result_set_metadata, "TABLE_SCHEMA").unwrap_or(0);
    let t_name_idx = column_index(&tables_resp.result_set_metadata, "TABLE_NAME").unwrap_or(1);
    let t_type_idx = column_index(&tables_resp.result_set_metadata, "TABLE_TYPE").unwrap_or(2);
    let t_comment_idx = column_index(&tables_resp.result_set_metadata, "COMMENT").unwrap_or(3);
    let t_row_count_idx =
        column_index(&tables_resp.result_set_metadata, "ROW_COUNT").unwrap_or(4);
    let t_bytes_idx = column_index(&tables_resp.result_set_metadata, "BYTES").unwrap_or(5);

    for row in &tables_resp.data {
        let schema_name = get_cell(row, t_schema_idx).unwrap_or("").to_string();
        let table_name = get_cell(row, t_name_idx).unwrap_or("").to_string();
        let table_type_str = get_cell(row, t_type_idx).unwrap_or("BASE TABLE");
        let comment = get_cell(row, t_comment_idx).map(|s| s.to_string());
        let row_count = get_cell(row, t_row_count_idx).and_then(|s| s.parse::<i64>().ok());
        let byte_size = get_cell(row, t_bytes_idx).and_then(|s| s.parse::<i64>().ok());

        let schema_id = match schema_map.get(&schema_name) {
            Some(&id) => id,
            None => continue, // skip tables in schemas we didn't sync
        };

        let table_type = match table_type_str {
            "VIEW" => TableType::View,
            "MATERIALIZED VIEW" => TableType::MaterializedView,
            "EXTERNAL TABLE" => TableType::ExternalTable,
            _ => TableType::BaseTable,
        };

        let now = Utc::now();
        let table_id = Uuid::new_v4();
        table_map.insert((schema_name.clone(), table_name.clone()), table_id);

        // Filter empty comments
        let description = comment.filter(|c| !c.is_empty());

        output.tables.push(Table {
            id: table_id,
            schema_id,
            name: table_name,
            table_type,
            description: description.clone(),
            dbt_model: None,
            owner: None,
            row_count,
            byte_size,
            confidence: 0.80,
            confidence_refreshed_at: Some(now),
            tags: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        });
        output.stats.tables_upserted += 1;

        // Create SemanticDefinition from Snowflake table comment
        if let Some(desc) = description {
            output.semantic_definitions.push(SemanticDefinition {
                id: Uuid::new_v4(),
                entity_id: table_id,
                entity_type: SemanticEntityType::Table,
                definition: desc,
                source: DefinitionSource::SnowflakeComment,
                confidence: 0.80,
                confidence_refreshed_at: Some(now),
                embedding: None,
                definition_hash: None,
                created_at: now,
                updated_at: now,
            });
        }
    }

    // 3. Sync columns
    let columns_resp = client
        .execute_sql(&format!(
            "SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME, DATA_TYPE, \
                    ORDINAL_POSITION, IS_NULLABLE, COMMENT \
             FROM {}.INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA NOT IN ('INFORMATION_SCHEMA') \
             ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION",
            config.database
        ))
        .await
        .context("failed to query INFORMATION_SCHEMA.COLUMNS")?;

    let c_schema_idx =
        column_index(&columns_resp.result_set_metadata, "TABLE_SCHEMA").unwrap_or(0);
    let c_table_idx =
        column_index(&columns_resp.result_set_metadata, "TABLE_NAME").unwrap_or(1);
    let c_name_idx =
        column_index(&columns_resp.result_set_metadata, "COLUMN_NAME").unwrap_or(2);
    let c_type_idx = column_index(&columns_resp.result_set_metadata, "DATA_TYPE").unwrap_or(3);
    let c_ordinal_idx =
        column_index(&columns_resp.result_set_metadata, "ORDINAL_POSITION").unwrap_or(4);
    let c_nullable_idx =
        column_index(&columns_resp.result_set_metadata, "IS_NULLABLE").unwrap_or(5);
    let c_comment_idx = column_index(&columns_resp.result_set_metadata, "COMMENT").unwrap_or(6);

    for row in &columns_resp.data {
        let schema_name = get_cell(row, c_schema_idx).unwrap_or("").to_string();
        let table_name = get_cell(row, c_table_idx).unwrap_or("").to_string();
        let col_name = get_cell(row, c_name_idx).unwrap_or("").to_string();
        let data_type = get_cell(row, c_type_idx).unwrap_or("VARCHAR").to_string();
        let ordinal = get_cell(row, c_ordinal_idx)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        let is_nullable = get_cell(row, c_nullable_idx) != Some("NO");
        let comment = get_cell(row, c_comment_idx)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        let Some(&table_id) = table_map.get(&(schema_name, table_name)) else {
            continue;
        };

        let now = Utc::now();
        let col_id = Uuid::new_v4();

        output.columns.push(Column {
            id: col_id,
            table_id,
            name: col_name,
            data_type,
            ordinal_position: ordinal,
            is_nullable,
            is_primary_key: false, // could enrich from TABLE_CONSTRAINTS later
            is_foreign_key: false,
            description: comment.clone(),
            dbt_meta: None,
            tags: serde_json::json!({}),
            confidence: 0.80,
            confidence_refreshed_at: Some(now),
            created_at: now,
            updated_at: now,
        });
        output.stats.columns_upserted += 1;

        // Create SemanticDefinition from Snowflake column comment
        if let Some(desc) = comment {
            output.semantic_definitions.push(SemanticDefinition {
                id: Uuid::new_v4(),
                entity_id: col_id,
                entity_type: SemanticEntityType::Column,
                definition: desc,
                source: DefinitionSource::SnowflakeComment,
                confidence: 0.80,
                confidence_refreshed_at: Some(now),
                embedding: None,
                definition_hash: None,
                created_at: now,
                updated_at: now,
            });
        }
    }

    tracing::info!(
        "Snowflake sync complete: {} schemas, {} tables, {} columns",
        output.stats.schemas_upserted,
        output.stats.tables_upserted,
        output.stats.columns_upserted,
    );

    Ok(output)
}
