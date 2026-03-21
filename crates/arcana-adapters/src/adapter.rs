use anyhow::Result;
use async_trait::async_trait;
use arcana_core::entities::{Column, LineageEdge, Metric, Schema, SemanticDefinition, Table};

/// Output of a metadata adapter sync operation.
#[derive(Debug, Default)]
pub struct SyncOutput {
    pub schemas: Vec<Schema>,
    pub tables: Vec<Table>,
    pub columns: Vec<Column>,
    pub lineage_edges: Vec<LineageEdge>,
    pub semantic_definitions: Vec<SemanticDefinition>,
    pub metrics: Vec<Metric>,
    pub stats: SyncStats,
    /// Changed checksums from incremental sync: (entity_key, new_checksum).
    pub changed_checksums: Vec<(String, String)>,
}

/// Counts of entities synced (for reporting).
#[derive(Debug, Default, Clone, Copy)]
pub struct SyncStats {
    pub schemas_upserted: usize,
    pub tables_upserted: usize,
    pub columns_upserted: usize,
    pub lineage_edges_upserted: usize,
    pub errors: usize,
}

/// A source of structured metadata (Snowflake, dbt, BigQuery, etc.).
///
/// Implementors discover and return entities that are then persisted by
/// the core metadata store.
#[async_trait]
pub trait MetadataAdapter: Send + Sync {
    /// Human-readable name of this adapter (e.g., "snowflake", "dbt").
    fn name(&self) -> &str;

    /// Perform a full (or incremental) metadata sync and return all discovered entities.
    async fn sync(&self) -> Result<SyncOutput>;

    /// Validate connectivity and permissions.
    async fn health_check(&self) -> Result<()>;
}
