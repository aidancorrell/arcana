use anyhow::Result;
use async_trait::async_trait;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

use crate::entities::{
    AgentInteraction, Column, ColumnProfile, DataContract, DataSource, Document, DocumentChunk,
    EntityLink, LineageEdge, Metric, Schema, SemanticDefinition, Table, UsageRecord,
};

use super::MetadataStore;

/// SQLite-backed implementation of the metadata store.
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at the given URL and run migrations.
    pub async fn open(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        // Run embedded migrations from the migrations/ directory.
        sqlx::migrate!("src/store/migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl MetadataStore for SqliteStore {
    async fn upsert_data_source(&self, ds: &DataSource) -> Result<()> {
        let id = ds.id.to_string();
        let source_type = serde_json::to_string(&ds.source_type)?;
        let connection_info = ds.connection_info.to_string();
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
        .bind(id)
        .bind(&ds.name)
        .bind(source_type)
        .bind(connection_info)
        .bind(ds.created_at)
        .bind(ds.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_data_source(&self, id: Uuid) -> Result<Option<DataSource>> {
        let _ = id;
        todo!("implement get_data_source for SQLite")
    }

    async fn list_data_sources(&self) -> Result<Vec<DataSource>> {
        todo!("implement list_data_sources for SQLite")
    }

    async fn upsert_schema(&self, schema: &Schema) -> Result<()> {
        let id = schema.id.to_string();
        let data_source_id = schema.data_source_id.to_string();
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
        .bind(id)
        .bind(data_source_id)
        .bind(&schema.database_name)
        .bind(&schema.schema_name)
        .bind(schema.created_at)
        .bind(schema.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_schemas(&self, data_source_id: Uuid) -> Result<Vec<Schema>> {
        let _ = data_source_id;
        todo!("implement list_schemas for SQLite")
    }

    async fn upsert_table(&self, table: &Table) -> Result<()> {
        let id = table.id.to_string();
        let schema_id = table.schema_id.to_string();
        let table_type = serde_json::to_string(&table.table_type)?;
        let tags = table.tags.to_string();
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
        .bind(id)
        .bind(schema_id)
        .bind(&table.name)
        .bind(table_type)
        .bind(&table.description)
        .bind(&table.dbt_model)
        .bind(&table.owner)
        .bind(table.row_count)
        .bind(table.byte_size)
        .bind(table.confidence)
        .bind(table.confidence_refreshed_at)
        .bind(tags)
        .bind(table.created_at)
        .bind(table.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_table(&self, id: Uuid) -> Result<Option<Table>> {
        let _ = id;
        todo!("implement get_table for SQLite")
    }

    async fn list_tables(&self, schema_id: Uuid) -> Result<Vec<Table>> {
        let _ = schema_id;
        todo!("implement list_tables for SQLite")
    }

    async fn search_tables(&self, query: &str, limit: u32) -> Result<Vec<Table>> {
        let _ = (query, limit);
        todo!("implement search_tables for SQLite (FTS5)")
    }

    async fn upsert_column(&self, column: &Column) -> Result<()> {
        let _ = column;
        todo!("implement upsert_column for SQLite")
    }

    async fn list_columns(&self, table_id: Uuid) -> Result<Vec<Column>> {
        let _ = table_id;
        todo!("implement list_columns for SQLite")
    }

    async fn upsert_column_profile(&self, profile: &ColumnProfile) -> Result<()> {
        let _ = profile;
        todo!("implement upsert_column_profile for SQLite")
    }

    async fn upsert_semantic_definition(&self, def: &SemanticDefinition) -> Result<()> {
        let _ = def;
        todo!("implement upsert_semantic_definition for SQLite")
    }

    async fn get_semantic_definitions(&self, entity_id: Uuid) -> Result<Vec<SemanticDefinition>> {
        let _ = entity_id;
        todo!("implement get_semantic_definitions for SQLite")
    }

    async fn upsert_metric(&self, metric: &Metric) -> Result<()> {
        let _ = metric;
        todo!("implement upsert_metric for SQLite")
    }

    async fn list_metrics(&self) -> Result<Vec<Metric>> {
        todo!("implement list_metrics for SQLite")
    }

    async fn upsert_contract(&self, contract: &DataContract) -> Result<()> {
        let _ = contract;
        todo!("implement upsert_contract for SQLite")
    }

    async fn list_contracts(&self, entity_id: Uuid) -> Result<Vec<DataContract>> {
        let _ = entity_id;
        todo!("implement list_contracts for SQLite")
    }

    async fn upsert_lineage_edge(&self, edge: &LineageEdge) -> Result<()> {
        let _ = edge;
        todo!("implement upsert_lineage_edge for SQLite")
    }

    async fn get_upstream(&self, entity_id: Uuid) -> Result<Vec<LineageEdge>> {
        let _ = entity_id;
        todo!("implement get_upstream for SQLite")
    }

    async fn get_downstream(&self, entity_id: Uuid) -> Result<Vec<LineageEdge>> {
        let _ = entity_id;
        todo!("implement get_downstream for SQLite")
    }

    async fn upsert_document(&self, doc: &Document) -> Result<()> {
        let _ = doc;
        todo!("implement upsert_document for SQLite")
    }

    async fn get_document(&self, id: Uuid) -> Result<Option<Document>> {
        let _ = id;
        todo!("implement get_document for SQLite")
    }

    async fn upsert_chunk(&self, chunk: &DocumentChunk) -> Result<()> {
        let _ = chunk;
        todo!("implement upsert_chunk for SQLite")
    }

    async fn list_chunks(&self, document_id: Uuid) -> Result<Vec<DocumentChunk>> {
        let _ = document_id;
        todo!("implement list_chunks for SQLite")
    }

    async fn upsert_entity_link(&self, link: &EntityLink) -> Result<()> {
        let _ = link;
        todo!("implement upsert_entity_link for SQLite")
    }

    async fn insert_usage_record(&self, record: &UsageRecord) -> Result<()> {
        let _ = record;
        todo!("implement insert_usage_record for SQLite")
    }

    async fn insert_agent_interaction(&self, interaction: &AgentInteraction) -> Result<()> {
        let _ = interaction;
        todo!("implement insert_agent_interaction for SQLite")
    }
}
