pub mod sqlite;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::{
    Column, ColumnProfile, DataContract, DataSource, Document, DocumentChunk, EntityLink,
    EvidenceRecord, LineageEdge, Metric, Schema, SemanticDefinition, Table, TableCluster,
    TableClusterMember, UsageRecord, AgentInteraction,
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
    async fn list_all_semantic_definitions(&self) -> Result<Vec<SemanticDefinition>>;
    /// BM25 full-text search over definition text. Returns (entity_id, score) pairs
    /// with scores normalized to [0, 1] (higher = more relevant).
    async fn fts_search(&self, query: &str, limit: u32) -> Result<Vec<(Uuid, f32)>>;

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

    // --- TableCluster (dedup) ---
    async fn upsert_table_cluster(&self, cluster: &TableCluster) -> Result<()>;
    async fn upsert_cluster_member(&self, member: &TableClusterMember) -> Result<()>;
    async fn get_cluster_for_table(&self, table_id: Uuid) -> Result<Option<(TableCluster, Vec<TableClusterMember>)>>;
    async fn list_table_clusters(&self) -> Result<Vec<TableCluster>>;
    async fn clear_table_clusters(&self) -> Result<()>;

    // --- Usage ---
    async fn insert_usage_record(&self, record: &UsageRecord) -> Result<()>;
    async fn insert_agent_interaction(&self, interaction: &AgentInteraction) -> Result<()>;
    async fn update_interaction_feedback(&self, id: Uuid, was_helpful: bool) -> Result<()>;

    // --- Evidence (feedback loop) ---
    async fn insert_evidence_record(&self, record: &EvidenceRecord) -> Result<()>;
    async fn get_evidence_for_entity(&self, entity_id: Uuid) -> Result<Vec<EvidenceRecord>>;
    /// Boost confidence on all semantic definitions for an entity by `delta`, clamped to [0.0, 1.0].
    async fn boost_confidence(&self, entity_id: Uuid, delta: f64) -> Result<()>;

    // --- Sync checksums (incremental sync) ---
    async fn get_sync_checksum(&self, adapter: &str, entity_key: &str) -> Result<Option<String>>;
    async fn upsert_sync_checksum(&self, adapter: &str, entity_key: &str, checksum: &str) -> Result<()>;
    async fn list_sync_checksums(&self, adapter: &str) -> Result<Vec<(String, String)>>;

    // --- Batch queries (avoids N+1 in admin API) ---
    /// Count tables and definition coverage across all schemas in a single query.
    async fn count_tables_and_coverage(&self) -> Result<TableCoverageStats>;
    /// List recent evidence records across all entities, ordered by created_at desc.
    async fn list_recent_evidence(&self, limit: u32) -> Result<Vec<EvidenceRecord>>;
}

/// Aggregated coverage statistics returned by `count_tables_and_coverage`.
#[derive(Debug, Default)]
pub struct TableCoverageStats {
    pub data_source_count: usize,
    pub total_tables: usize,
    pub tables_with_definitions: usize,
    pub total_columns: usize,
    pub schema_coverages: Vec<SchemaCoverageRow>,
}

/// Per-schema coverage row.
#[derive(Debug)]
pub struct SchemaCoverageRow {
    pub data_source_name: String,
    pub database_name: String,
    pub schema_name: String,
    pub total_tables: usize,
    pub tables_with_definitions: usize,
}

pub use sqlite::SqliteStore;
