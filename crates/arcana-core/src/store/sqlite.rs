use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool, sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions}};
use std::str::FromStr;
use uuid::Uuid;

use crate::entities::{
    AgentInteraction, Column, ColumnProfile, ContractEntityType, ContractResult, ContractStatus,
    ContractType, DataContract, DataSource, DataSourceType, Document, DocumentChunk,
    DocumentSourceType, EntityLink, LineageEdge, LineageNodeType, LineageSource,
    LinkedEntityType, LinkMethod, Metric, MetricType, QueryType, Schema, SemanticDefinition,
    SemanticEntityType, DefinitionSource, Table, TableType, UsageRecord,
};

use super::MetadataStore;

/// SQLite-backed implementation of the metadata store.
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at the given URL and run migrations.
    pub async fn open(database_url: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::from_str(database_url)
            .context("invalid database URL")?
            .journal_mode(SqliteJournalMode::Wal)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("src/store/migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_uuid(s: &str) -> Result<Uuid> {
    Uuid::parse_str(s).with_context(|| format!("invalid UUID: {s}"))
}

fn json_str<T: serde::Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).context("failed to serialize to JSON")
}

fn from_json_str<T: serde::de::DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_str(s).with_context(|| format!("failed to deserialize JSON: {s}"))
}

// ---------------------------------------------------------------------------
// Row → Entity mappers
// ---------------------------------------------------------------------------

fn row_to_data_source(row: &sqlx::sqlite::SqliteRow) -> Result<DataSource> {
    let id: String = row.try_get("id")?;
    let source_type: String = row.try_get("source_type")?;
    let connection_info: String = row.try_get("connection_info")?;
    Ok(DataSource {
        id: parse_uuid(&id)?,
        name: row.try_get("name")?,
        source_type: from_json_str(&source_type)?,
        connection_info: from_json_str(&connection_info)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_schema(row: &sqlx::sqlite::SqliteRow) -> Result<Schema> {
    let id: String = row.try_get("id")?;
    let data_source_id: String = row.try_get("data_source_id")?;
    Ok(Schema {
        id: parse_uuid(&id)?,
        data_source_id: parse_uuid(&data_source_id)?,
        database_name: row.try_get("database_name")?,
        schema_name: row.try_get("schema_name")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_table(row: &sqlx::sqlite::SqliteRow) -> Result<Table> {
    let id: String = row.try_get("id")?;
    let schema_id: String = row.try_get("schema_id")?;
    let table_type: String = row.try_get("table_type")?;
    let tags: String = row.try_get("tags")?;
    Ok(Table {
        id: parse_uuid(&id)?,
        schema_id: parse_uuid(&schema_id)?,
        name: row.try_get("name")?,
        table_type: from_json_str(&table_type)?,
        description: row.try_get("description")?,
        dbt_model: row.try_get("dbt_model")?,
        owner: row.try_get("owner")?,
        row_count: row.try_get("row_count")?,
        byte_size: row.try_get("byte_size")?,
        confidence: row.try_get("confidence")?,
        confidence_refreshed_at: row.try_get("confidence_refreshed_at")?,
        tags: from_json_str(&tags)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_column(row: &sqlx::sqlite::SqliteRow) -> Result<Column> {
    let id: String = row.try_get("id")?;
    let table_id: String = row.try_get("table_id")?;
    let dbt_meta: Option<String> = row.try_get("dbt_meta")?;
    let tags: String = row.try_get("tags")?;
    Ok(Column {
        id: parse_uuid(&id)?,
        table_id: parse_uuid(&table_id)?,
        name: row.try_get("name")?,
        data_type: row.try_get("data_type")?,
        ordinal_position: row.try_get("ordinal_position")?,
        is_nullable: row.try_get("is_nullable")?,
        is_primary_key: row.try_get("is_primary_key")?,
        is_foreign_key: row.try_get("is_foreign_key")?,
        description: row.try_get("description")?,
        dbt_meta: dbt_meta.as_deref().map(serde_json::from_str).transpose()?,
        tags: from_json_str(&tags)?,
        confidence: row.try_get("confidence")?,
        confidence_refreshed_at: row.try_get("confidence_refreshed_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_column_profile(row: &sqlx::sqlite::SqliteRow) -> Result<ColumnProfile> {
    let id: String = row.try_get("id")?;
    let column_id: String = row.try_get("column_id")?;
    let min_value: Option<String> = row.try_get("min_value")?;
    let max_value: Option<String> = row.try_get("max_value")?;
    let top_values: Option<String> = row.try_get("top_values")?;
    Ok(ColumnProfile {
        id: parse_uuid(&id)?,
        column_id: parse_uuid(&column_id)?,
        null_count: row.try_get("null_count")?,
        null_pct: row.try_get("null_pct")?,
        distinct_count: row.try_get("distinct_count")?,
        min_value: min_value.as_deref().map(serde_json::from_str).transpose()?,
        max_value: max_value.as_deref().map(serde_json::from_str).transpose()?,
        mean_value: row.try_get("mean_value")?,
        stddev_value: row.try_get("stddev_value")?,
        top_values: top_values.as_deref().map(serde_json::from_str).transpose()?,
        profiled_at: row.try_get("profiled_at")?,
    })
}

fn row_to_semantic_definition(row: &sqlx::sqlite::SqliteRow) -> Result<SemanticDefinition> {
    let id: String = row.try_get("id")?;
    let entity_id: String = row.try_get("entity_id")?;
    let entity_type: String = row.try_get("entity_type")?;
    let source: String = row.try_get("source")?;
    let embedding: Option<String> = row.try_get("embedding")?;
    Ok(SemanticDefinition {
        id: parse_uuid(&id)?,
        entity_id: parse_uuid(&entity_id)?,
        entity_type: from_json_str(&entity_type)?,
        definition: row.try_get("definition")?,
        source: from_json_str(&source)?,
        confidence: row.try_get("confidence")?,
        confidence_refreshed_at: row.try_get("confidence_refreshed_at")?,
        embedding: embedding.as_deref().map(serde_json::from_str).transpose()?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_metric(row: &sqlx::sqlite::SqliteRow) -> Result<Metric> {
    let id: String = row.try_get("id")?;
    let metric_type: String = row.try_get("metric_type")?;
    let source_table_id: Option<String> = row.try_get("source_table_id")?;
    let dimensions: String = row.try_get("dimensions")?;
    let filters: Option<String> = row.try_get("filters")?;
    Ok(Metric {
        id: parse_uuid(&id)?,
        name: row.try_get("name")?,
        label: row.try_get("label")?,
        description: row.try_get("description")?,
        metric_type: from_json_str(&metric_type)?,
        source_table_id: source_table_id.as_deref().map(parse_uuid).transpose()?,
        expression: row.try_get("expression")?,
        dimensions: from_json_str(&dimensions)?,
        filters: filters.as_deref().map(serde_json::from_str).transpose()?,
        confidence: row.try_get("confidence")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_data_contract(row: &sqlx::sqlite::SqliteRow) -> Result<DataContract> {
    let id: String = row.try_get("id")?;
    let entity_id: String = row.try_get("entity_id")?;
    let entity_type: String = row.try_get("entity_type")?;
    let contract_type: String = row.try_get("contract_type")?;
    let expression: String = row.try_get("expression")?;
    let status: String = row.try_get("status")?;
    let last_result: Option<String> = row.try_get("last_result")?;
    Ok(DataContract {
        id: parse_uuid(&id)?,
        name: row.try_get("name")?,
        entity_id: parse_uuid(&entity_id)?,
        entity_type: from_json_str(&entity_type)?,
        contract_type: from_json_str(&contract_type)?,
        description: row.try_get("description")?,
        expression: from_json_str(&expression)?,
        status: from_json_str(&status)?,
        last_evaluated_at: row.try_get("last_evaluated_at")?,
        last_result: last_result.as_deref().map(serde_json::from_str).transpose()?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_lineage_edge(row: &sqlx::sqlite::SqliteRow) -> Result<LineageEdge> {
    let id: String = row.try_get("id")?;
    let upstream_id: String = row.try_get("upstream_id")?;
    let upstream_type: String = row.try_get("upstream_type")?;
    let downstream_id: String = row.try_get("downstream_id")?;
    let downstream_type: String = row.try_get("downstream_type")?;
    let source: String = row.try_get("source")?;
    Ok(LineageEdge {
        id: parse_uuid(&id)?,
        upstream_id: parse_uuid(&upstream_id)?,
        upstream_type: from_json_str(&upstream_type)?,
        downstream_id: parse_uuid(&downstream_id)?,
        downstream_type: from_json_str(&downstream_type)?,
        source: from_json_str(&source)?,
        transform_expression: row.try_get("transform_expression")?,
        confidence: row.try_get("confidence")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_document(row: &sqlx::sqlite::SqliteRow) -> Result<Document> {
    let id: String = row.try_get("id")?;
    let source_type: String = row.try_get("source_type")?;
    Ok(Document {
        id: parse_uuid(&id)?,
        title: row.try_get("title")?,
        source_type: from_json_str(&source_type)?,
        source_uri: row.try_get("source_uri")?,
        raw_content: row.try_get("raw_content")?,
        content: row.try_get("content")?,
        content_hash: row.try_get("content_hash")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_document_chunk(row: &sqlx::sqlite::SqliteRow) -> Result<DocumentChunk> {
    let id: String = row.try_get("id")?;
    let document_id: String = row.try_get("document_id")?;
    let section_path: String = row.try_get("section_path")?;
    let embedding: Option<String> = row.try_get("embedding")?;
    Ok(DocumentChunk {
        id: parse_uuid(&id)?,
        document_id: parse_uuid(&document_id)?,
        chunk_index: row.try_get("chunk_index")?,
        content: row.try_get("content")?,
        char_start: row.try_get("char_start")?,
        char_end: row.try_get("char_end")?,
        section_path: from_json_str(&section_path)?,
        embedding: embedding.as_deref().map(serde_json::from_str).transpose()?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_entity_link(row: &sqlx::sqlite::SqliteRow) -> Result<EntityLink> {
    let id: String = row.try_get("id")?;
    let chunk_id: String = row.try_get("chunk_id")?;
    let entity_id: String = row.try_get("entity_id")?;
    let entity_type: String = row.try_get("entity_type")?;
    let link_method: String = row.try_get("link_method")?;
    Ok(EntityLink {
        id: parse_uuid(&id)?,
        chunk_id: parse_uuid(&chunk_id)?,
        entity_id: parse_uuid(&entity_id)?,
        entity_type: from_json_str(&entity_type)?,
        link_method: from_json_str(&link_method)?,
        confidence: row.try_get("confidence")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_usage_record(row: &sqlx::sqlite::SqliteRow) -> Result<UsageRecord> {
    let id: String = row.try_get("id")?;
    let table_id: String = row.try_get("table_id")?;
    let query_type: String = row.try_get("query_type")?;
    Ok(UsageRecord {
        id: parse_uuid(&id)?,
        table_id: parse_uuid(&table_id)?,
        actor: row.try_get("actor")?,
        warehouse: row.try_get("warehouse")?,
        query_type: from_json_str(&query_type)?,
        bytes_scanned: row.try_get("bytes_scanned")?,
        credits_used: row.try_get("credits_used")?,
        duration_ms: row.try_get("duration_ms")?,
        executed_at: row.try_get("executed_at")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_agent_interaction(row: &sqlx::sqlite::SqliteRow) -> Result<AgentInteraction> {
    let id: String = row.try_get("id")?;
    let input: String = row.try_get("input")?;
    let referenced_entity_ids: String = row.try_get("referenced_entity_ids")?;
    let was_helpful: Option<bool> = row.try_get("was_helpful")?;
    Ok(AgentInteraction {
        id: parse_uuid(&id)?,
        tool_name: row.try_get("tool_name")?,
        input: from_json_str(&input)?,
        referenced_entity_ids: {
            let ids: Vec<String> = from_json_str(&referenced_entity_ids)?;
            ids.iter().map(|s| parse_uuid(s)).collect::<Result<Vec<_>>>()?
        },
        agent_id: row.try_get("agent_id")?,
        was_helpful,
        latency_ms: row.try_get("latency_ms")?,
        created_at: row.try_get("created_at")?,
    })
}

// ---------------------------------------------------------------------------
// MetadataStore implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl MetadataStore for SqliteStore {
    // --- DataSource ---

    async fn upsert_data_source(&self, ds: &DataSource) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO data_sources (id, name, source_type, connection_info, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                source_type = excluded.source_type,
                connection_info = excluded.connection_info,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(ds.id.to_string())
        .bind(&ds.name)
        .bind(json_str(&ds.source_type)?)
        .bind(ds.connection_info.to_string())
        .bind(ds.created_at)
        .bind(ds.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_data_source(&self, id: Uuid) -> Result<Option<DataSource>> {
        let row = sqlx::query("SELECT * FROM data_sources WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| row_to_data_source(&r)).transpose()
    }

    async fn list_data_sources(&self) -> Result<Vec<DataSource>> {
        let rows = sqlx::query("SELECT * FROM data_sources ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_data_source).collect()
    }

    // --- Schema ---

    async fn upsert_schema(&self, schema: &Schema) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO schemas (id, data_source_id, database_name, schema_name, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                database_name = excluded.database_name,
                schema_name = excluded.schema_name,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(schema.id.to_string())
        .bind(schema.data_source_id.to_string())
        .bind(&schema.database_name)
        .bind(&schema.schema_name)
        .bind(schema.created_at)
        .bind(schema.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_schemas(&self, data_source_id: Uuid) -> Result<Vec<Schema>> {
        let rows = sqlx::query(
            "SELECT * FROM schemas WHERE data_source_id = ? ORDER BY database_name, schema_name",
        )
        .bind(data_source_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_schema).collect()
    }

    // --- Table ---

    async fn upsert_table(&self, table: &Table) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO tables (
                id, schema_id, name, table_type, description, dbt_model,
                owner, row_count, byte_size, confidence, confidence_refreshed_at,
                tags, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                table_type = excluded.table_type,
                description = excluded.description,
                dbt_model = excluded.dbt_model,
                owner = excluded.owner,
                row_count = excluded.row_count,
                byte_size = excluded.byte_size,
                confidence = excluded.confidence,
                confidence_refreshed_at = excluded.confidence_refreshed_at,
                tags = excluded.tags,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(table.id.to_string())
        .bind(table.schema_id.to_string())
        .bind(&table.name)
        .bind(json_str(&table.table_type)?)
        .bind(&table.description)
        .bind(&table.dbt_model)
        .bind(&table.owner)
        .bind(table.row_count)
        .bind(table.byte_size)
        .bind(table.confidence)
        .bind(table.confidence_refreshed_at)
        .bind(table.tags.to_string())
        .bind(table.created_at)
        .bind(table.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_table(&self, id: Uuid) -> Result<Option<Table>> {
        let row = sqlx::query("SELECT * FROM tables WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| row_to_table(&r)).transpose()
    }

    async fn list_tables(&self, schema_id: Uuid) -> Result<Vec<Table>> {
        let rows = sqlx::query("SELECT * FROM tables WHERE schema_id = ? ORDER BY name")
            .bind(schema_id.to_string())
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_table).collect()
    }

    async fn search_tables(&self, query: &str, limit: u32) -> Result<Vec<Table>> {
        let pattern = format!("%{query}%");
        let rows = sqlx::query(
            r#"
            SELECT * FROM tables
            WHERE name LIKE ? OR description LIKE ?
            ORDER BY confidence DESC, name
            LIMIT ?
            "#,
        )
        .bind(&pattern)
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_table).collect()
    }

    // --- Column ---

    async fn upsert_column(&self, col: &Column) -> Result<()> {
        let dbt_meta = col.dbt_meta.as_ref().map(|v| v.to_string());
        sqlx::query(
            r#"
            INSERT INTO columns (
                id, table_id, name, data_type, ordinal_position,
                is_nullable, is_primary_key, is_foreign_key,
                description, dbt_meta, tags, confidence, confidence_refreshed_at,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                data_type = excluded.data_type,
                ordinal_position = excluded.ordinal_position,
                is_nullable = excluded.is_nullable,
                is_primary_key = excluded.is_primary_key,
                is_foreign_key = excluded.is_foreign_key,
                description = excluded.description,
                dbt_meta = excluded.dbt_meta,
                tags = excluded.tags,
                confidence = excluded.confidence,
                confidence_refreshed_at = excluded.confidence_refreshed_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(col.id.to_string())
        .bind(col.table_id.to_string())
        .bind(&col.name)
        .bind(&col.data_type)
        .bind(col.ordinal_position)
        .bind(col.is_nullable)
        .bind(col.is_primary_key)
        .bind(col.is_foreign_key)
        .bind(&col.description)
        .bind(dbt_meta)
        .bind(col.tags.to_string())
        .bind(col.confidence)
        .bind(col.confidence_refreshed_at)
        .bind(col.created_at)
        .bind(col.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_columns(&self, table_id: Uuid) -> Result<Vec<Column>> {
        let rows = sqlx::query(
            "SELECT * FROM columns WHERE table_id = ? ORDER BY ordinal_position",
        )
        .bind(table_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_column).collect()
    }

    // --- ColumnProfile ---

    async fn upsert_column_profile(&self, profile: &ColumnProfile) -> Result<()> {
        let min_value = profile.min_value.as_ref().map(|v| v.to_string());
        let max_value = profile.max_value.as_ref().map(|v| v.to_string());
        let top_values = profile.top_values.as_ref().map(|v| v.to_string());
        sqlx::query(
            r#"
            INSERT INTO column_profiles (
                id, column_id, null_count, null_pct, distinct_count,
                min_value, max_value, mean_value, stddev_value, top_values, profiled_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                null_count = excluded.null_count,
                null_pct = excluded.null_pct,
                distinct_count = excluded.distinct_count,
                min_value = excluded.min_value,
                max_value = excluded.max_value,
                mean_value = excluded.mean_value,
                stddev_value = excluded.stddev_value,
                top_values = excluded.top_values,
                profiled_at = excluded.profiled_at
            "#,
        )
        .bind(profile.id.to_string())
        .bind(profile.column_id.to_string())
        .bind(profile.null_count)
        .bind(profile.null_pct)
        .bind(profile.distinct_count)
        .bind(min_value)
        .bind(max_value)
        .bind(profile.mean_value)
        .bind(profile.stddev_value)
        .bind(top_values)
        .bind(profile.profiled_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- SemanticDefinition ---

    async fn upsert_semantic_definition(&self, def: &SemanticDefinition) -> Result<()> {
        let embedding = def.embedding.as_ref().map(|e| json_str(e)).transpose()?;
        sqlx::query(
            r#"
            INSERT INTO semantic_definitions (
                id, entity_id, entity_type, definition, source,
                confidence, confidence_refreshed_at, embedding, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                definition = excluded.definition,
                source = excluded.source,
                confidence = excluded.confidence,
                confidence_refreshed_at = excluded.confidence_refreshed_at,
                embedding = excluded.embedding,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(def.id.to_string())
        .bind(def.entity_id.to_string())
        .bind(json_str(&def.entity_type)?)
        .bind(&def.definition)
        .bind(json_str(&def.source)?)
        .bind(def.confidence)
        .bind(def.confidence_refreshed_at)
        .bind(embedding)
        .bind(def.created_at)
        .bind(def.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_semantic_definitions(&self, entity_id: Uuid) -> Result<Vec<SemanticDefinition>> {
        let rows = sqlx::query(
            "SELECT * FROM semantic_definitions WHERE entity_id = ? ORDER BY confidence DESC",
        )
        .bind(entity_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_semantic_definition).collect()
    }

    async fn list_all_semantic_definitions(&self) -> Result<Vec<SemanticDefinition>> {
        let rows = sqlx::query(
            "SELECT * FROM semantic_definitions ORDER BY confidence DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_semantic_definition).collect()
    }

    // --- Metric ---

    async fn upsert_metric(&self, metric: &Metric) -> Result<()> {
        let source_table_id = metric.source_table_id.map(|id| id.to_string());
        let filters = metric.filters.as_ref().map(|v| v.to_string());
        sqlx::query(
            r#"
            INSERT INTO metrics (
                id, name, label, description, metric_type,
                source_table_id, expression, dimensions, filters, confidence,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                label = excluded.label,
                description = excluded.description,
                metric_type = excluded.metric_type,
                source_table_id = excluded.source_table_id,
                expression = excluded.expression,
                dimensions = excluded.dimensions,
                filters = excluded.filters,
                confidence = excluded.confidence,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(metric.id.to_string())
        .bind(&metric.name)
        .bind(&metric.label)
        .bind(&metric.description)
        .bind(json_str(&metric.metric_type)?)
        .bind(source_table_id)
        .bind(&metric.expression)
        .bind(json_str(&metric.dimensions)?)
        .bind(filters)
        .bind(metric.confidence)
        .bind(metric.created_at)
        .bind(metric.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_metrics(&self) -> Result<Vec<Metric>> {
        let rows = sqlx::query("SELECT * FROM metrics ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_metric).collect()
    }

    // --- DataContract ---

    async fn upsert_contract(&self, contract: &DataContract) -> Result<()> {
        let last_result = contract.last_result.as_ref().map(json_str).transpose()?;
        sqlx::query(
            r#"
            INSERT INTO data_contracts (
                id, name, entity_id, entity_type, contract_type,
                description, expression, status, last_evaluated_at, last_result,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                entity_type = excluded.entity_type,
                contract_type = excluded.contract_type,
                description = excluded.description,
                expression = excluded.expression,
                status = excluded.status,
                last_evaluated_at = excluded.last_evaluated_at,
                last_result = excluded.last_result,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(contract.id.to_string())
        .bind(&contract.name)
        .bind(contract.entity_id.to_string())
        .bind(json_str(&contract.entity_type)?)
        .bind(json_str(&contract.contract_type)?)
        .bind(&contract.description)
        .bind(contract.expression.to_string())
        .bind(json_str(&contract.status)?)
        .bind(contract.last_evaluated_at)
        .bind(last_result)
        .bind(contract.created_at)
        .bind(contract.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_contracts(&self, entity_id: Uuid) -> Result<Vec<DataContract>> {
        let rows = sqlx::query(
            "SELECT * FROM data_contracts WHERE entity_id = ? ORDER BY created_at",
        )
        .bind(entity_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_data_contract).collect()
    }

    // --- LineageEdge ---

    async fn upsert_lineage_edge(&self, edge: &LineageEdge) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO lineage_edges (
                id, upstream_id, upstream_type, downstream_id, downstream_type,
                source, transform_expression, confidence, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(upstream_id, downstream_id) DO UPDATE SET
                upstream_type = excluded.upstream_type,
                downstream_type = excluded.downstream_type,
                source = excluded.source,
                transform_expression = excluded.transform_expression,
                confidence = excluded.confidence,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(edge.id.to_string())
        .bind(edge.upstream_id.to_string())
        .bind(json_str(&edge.upstream_type)?)
        .bind(edge.downstream_id.to_string())
        .bind(json_str(&edge.downstream_type)?)
        .bind(json_str(&edge.source)?)
        .bind(&edge.transform_expression)
        .bind(edge.confidence)
        .bind(edge.created_at)
        .bind(edge.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_upstream(&self, entity_id: Uuid) -> Result<Vec<LineageEdge>> {
        let rows = sqlx::query(
            "SELECT * FROM lineage_edges WHERE downstream_id = ? ORDER BY confidence DESC",
        )
        .bind(entity_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_lineage_edge).collect()
    }

    async fn get_downstream(&self, entity_id: Uuid) -> Result<Vec<LineageEdge>> {
        let rows = sqlx::query(
            "SELECT * FROM lineage_edges WHERE upstream_id = ? ORDER BY confidence DESC",
        )
        .bind(entity_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_lineage_edge).collect()
    }

    // --- Document ---

    async fn upsert_document(&self, doc: &Document) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO documents (
                id, title, source_type, source_uri, raw_content,
                content, content_hash, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(source_uri) DO UPDATE SET
                title = excluded.title,
                source_type = excluded.source_type,
                raw_content = excluded.raw_content,
                content = excluded.content,
                content_hash = excluded.content_hash,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(doc.id.to_string())
        .bind(&doc.title)
        .bind(json_str(&doc.source_type)?)
        .bind(&doc.source_uri)
        .bind(&doc.raw_content)
        .bind(&doc.content)
        .bind(&doc.content_hash)
        .bind(doc.created_at)
        .bind(doc.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_document(&self, id: Uuid) -> Result<Option<Document>> {
        let row = sqlx::query("SELECT * FROM documents WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| row_to_document(&r)).transpose()
    }

    // --- DocumentChunk ---

    async fn upsert_chunk(&self, chunk: &DocumentChunk) -> Result<()> {
        let embedding = chunk.embedding.as_ref().map(|e| json_str(e)).transpose()?;
        sqlx::query(
            r#"
            INSERT INTO document_chunks (
                id, document_id, chunk_index, content,
                char_start, char_end, section_path, embedding, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(document_id, chunk_index) DO UPDATE SET
                content = excluded.content,
                char_start = excluded.char_start,
                char_end = excluded.char_end,
                section_path = excluded.section_path,
                embedding = excluded.embedding
            "#,
        )
        .bind(chunk.id.to_string())
        .bind(chunk.document_id.to_string())
        .bind(chunk.chunk_index)
        .bind(&chunk.content)
        .bind(chunk.char_start)
        .bind(chunk.char_end)
        .bind(json_str(&chunk.section_path)?)
        .bind(embedding)
        .bind(chunk.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_chunks(&self, document_id: Uuid) -> Result<Vec<DocumentChunk>> {
        let rows = sqlx::query(
            "SELECT * FROM document_chunks WHERE document_id = ? ORDER BY chunk_index",
        )
        .bind(document_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_document_chunk).collect()
    }

    // --- EntityLink ---

    async fn upsert_entity_link(&self, link: &EntityLink) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO entity_links (
                id, chunk_id, entity_id, entity_type, link_method, confidence, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                link_method = excluded.link_method,
                confidence = excluded.confidence
            "#,
        )
        .bind(link.id.to_string())
        .bind(link.chunk_id.to_string())
        .bind(link.entity_id.to_string())
        .bind(json_str(&link.entity_type)?)
        .bind(json_str(&link.link_method)?)
        .bind(link.confidence)
        .bind(link.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- UsageRecord ---

    async fn insert_usage_record(&self, record: &UsageRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO usage_records (
                id, table_id, actor, warehouse, query_type,
                bytes_scanned, credits_used, duration_ms, executed_at, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(record.id.to_string())
        .bind(record.table_id.to_string())
        .bind(&record.actor)
        .bind(&record.warehouse)
        .bind(json_str(&record.query_type)?)
        .bind(record.bytes_scanned)
        .bind(record.credits_used)
        .bind(record.duration_ms)
        .bind(record.executed_at)
        .bind(record.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- AgentInteraction ---

    async fn insert_agent_interaction(&self, interaction: &AgentInteraction) -> Result<()> {
        let referenced_ids: Vec<String> = interaction
            .referenced_entity_ids
            .iter()
            .map(|id| id.to_string())
            .collect();
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO agent_interactions (
                id, tool_name, input, referenced_entity_ids,
                agent_id, was_helpful, latency_ms, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(interaction.id.to_string())
        .bind(&interaction.tool_name)
        .bind(interaction.input.to_string())
        .bind(json_str(&referenced_ids)?)
        .bind(&interaction.agent_id)
        .bind(interaction.was_helpful)
        .bind(interaction.latency_ms)
        .bind(interaction.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_interaction_feedback(&self, id: Uuid, was_helpful: bool) -> Result<()> {
        sqlx::query("UPDATE agent_interactions SET was_helpful = ? WHERE id = ?")
            .bind(was_helpful)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{DataSourceType, TableType};

    async fn test_store() -> SqliteStore {
        SqliteStore::open("sqlite::memory:").await.unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[tokio::test]
    async fn test_data_source_roundtrip() {
        let store = test_store().await;
        let ds = DataSource {
            id: Uuid::new_v4(),
            name: "test_snowflake".into(),
            source_type: DataSourceType::Snowflake,
            connection_info: serde_json::json!({"account": "xy12345"}),
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_data_source(&ds).await.unwrap();
        let fetched = store.get_data_source(ds.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, ds.id);
        assert_eq!(fetched.name, ds.name);
        assert_eq!(fetched.source_type, DataSourceType::Snowflake);

        let all = store.list_data_sources().await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_schema_and_table_roundtrip() {
        let store = test_store().await;

        let ds = DataSource {
            id: Uuid::new_v4(),
            name: "ds".into(),
            source_type: DataSourceType::Snowflake,
            connection_info: serde_json::json!({}),
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_data_source(&ds).await.unwrap();

        let schema = Schema {
            id: Uuid::new_v4(),
            data_source_id: ds.id,
            database_name: "ANALYTICS".into(),
            schema_name: "PUBLIC".into(),
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_schema(&schema).await.unwrap();

        let schemas = store.list_schemas(ds.id).await.unwrap();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].schema_name, "PUBLIC");

        let table = Table {
            id: Uuid::new_v4(),
            schema_id: schema.id,
            name: "fct_orders".into(),
            table_type: TableType::BaseTable,
            description: Some("One row per order".into()),
            dbt_model: Some("fct_orders".into()),
            owner: Some("data-team".into()),
            row_count: Some(1_000_000),
            byte_size: Some(500_000_000),
            confidence: 0.8,
            confidence_refreshed_at: None,
            tags: serde_json::json!({}),
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_table(&table).await.unwrap();

        let tables = store.list_tables(schema.id).await.unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "fct_orders");
        assert_eq!(tables[0].row_count, Some(1_000_000));

        let fetched = store.get_table(table.id).await.unwrap().unwrap();
        assert_eq!(fetched.description.as_deref(), Some("One row per order"));

        let results = store.search_tables("orders", 10).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_column_roundtrip() {
        let store = test_store().await;

        let ds = DataSource { id: Uuid::new_v4(), name: "ds".into(), source_type: DataSourceType::Dbt, connection_info: serde_json::json!({}), created_at: now(), updated_at: now() };
        store.upsert_data_source(&ds).await.unwrap();
        let schema = Schema { id: Uuid::new_v4(), data_source_id: ds.id, database_name: "db".into(), schema_name: "s".into(), created_at: now(), updated_at: now() };
        store.upsert_schema(&schema).await.unwrap();
        let table = Table { id: Uuid::new_v4(), schema_id: schema.id, name: "t".into(), table_type: TableType::View, description: None, dbt_model: None, owner: None, row_count: None, byte_size: None, confidence: 1.0, confidence_refreshed_at: None, tags: serde_json::json!({}), created_at: now(), updated_at: now() };
        store.upsert_table(&table).await.unwrap();

        let col = Column {
            id: Uuid::new_v4(),
            table_id: table.id,
            name: "revenue_usd".into(),
            data_type: "DECIMAL(12,2)".into(),
            ordinal_position: 1,
            is_nullable: false,
            is_primary_key: false,
            is_foreign_key: false,
            description: Some("Net revenue after discounts".into()),
            dbt_meta: None,
            tags: serde_json::json!({}),
            confidence: 0.9,
            confidence_refreshed_at: None,
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_column(&col).await.unwrap();

        let cols = store.list_columns(table.id).await.unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].name, "revenue_usd");
        assert!(!cols[0].is_nullable);
    }

    #[tokio::test]
    async fn test_semantic_definition_roundtrip() {
        let store = test_store().await;
        let entity_id = Uuid::new_v4();
        let def = SemanticDefinition {
            id: Uuid::new_v4(),
            entity_id,
            entity_type: SemanticEntityType::Table,
            definition: "Grain: one row per order".into(),
            source: DefinitionSource::DbtYaml,
            confidence: 0.85,
            confidence_refreshed_at: Some(now()),
            embedding: Some(vec![0.1, 0.2, 0.3]),
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_semantic_definition(&def).await.unwrap();

        let defs = store.get_semantic_definitions(entity_id).await.unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].definition, "Grain: one row per order");
        assert_eq!(defs[0].embedding.as_ref().unwrap().len(), 3);
    }
}
