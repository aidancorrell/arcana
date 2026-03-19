pub mod sqlite;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::{
    Column, ColumnProfile, DataContract, DataSource, Document, DocumentChunk, EntityLink,
    LineageEdge, Metric, Schema, SemanticDefinition, Table, UsageRecord, AgentInteraction,
};

/// The primary metadata store trait — implemented by SQLite (and later PostgreSQL).
#[async_trait]
pub trait MetadataStore: Send + Sync {
    // --- DataSource ---
    async fn upsert_data_source(&self, ds: &DataSource) -> Result<()>;
    async fn get_data_source(&self, id: Uuid) -> Result<Option<DataSource>>;
    async fn list_data_sources(&self) -> Result<Vec<DataSource>>;

    // --- Schema ---
    async fn upsert_schema(&self, schema: &Schema) -> Result<()>;
    async fn list_schemas(&self, data_source_id: Uuid) -> Result<Vec<Schema>>;

    // --- Table ---
    async fn upsert_table(&self, table: &Table) -> Result<()>;
    async fn get_table(&self, id: Uuid) -> Result<Option<Table>>;
    async fn list_tables(&self, schema_id: Uuid) -> Result<Vec<Table>>;
    async fn search_tables(&self, query: &str, limit: u32) -> Result<Vec<Table>>;

    // --- Column ---
    async fn upsert_column(&self, column: &Column) -> Result<()>;
    async fn list_columns(&self, table_id: Uuid) -> Result<Vec<Column>>;
    async fn upsert_column_profile(&self, profile: &ColumnProfile) -> Result<()>;

    // --- SemanticDefinition ---
    async fn upsert_semantic_definition(&self, def: &SemanticDefinition) -> Result<()>;
    async fn get_semantic_definitions(&self, entity_id: Uuid) -> Result<Vec<SemanticDefinition>>;

    // --- Metric ---
    async fn upsert_metric(&self, metric: &Metric) -> Result<()>;
    async fn list_metrics(&self) -> Result<Vec<Metric>>;

    // --- DataContract ---
    async fn upsert_contract(&self, contract: &DataContract) -> Result<()>;
    async fn list_contracts(&self, entity_id: Uuid) -> Result<Vec<DataContract>>;

    // --- LineageEdge ---
    async fn upsert_lineage_edge(&self, edge: &LineageEdge) -> Result<()>;
    async fn get_upstream(&self, entity_id: Uuid) -> Result<Vec<LineageEdge>>;
    async fn get_downstream(&self, entity_id: Uuid) -> Result<Vec<LineageEdge>>;

    // --- Document ---
    async fn upsert_document(&self, doc: &Document) -> Result<()>;
    async fn get_document(&self, id: Uuid) -> Result<Option<Document>>;
    async fn upsert_chunk(&self, chunk: &DocumentChunk) -> Result<()>;
    async fn list_chunks(&self, document_id: Uuid) -> Result<Vec<DocumentChunk>>;
    async fn upsert_entity_link(&self, link: &EntityLink) -> Result<()>;

    // --- Usage ---
    async fn insert_usage_record(&self, record: &UsageRecord) -> Result<()>;
    async fn insert_agent_interaction(&self, interaction: &AgentInteraction) -> Result<()>;
}

pub use sqlite::SqliteStore;
