use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A registered data source (e.g., a Snowflake account or dbt project).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub id: Uuid,
    pub name: String,
    pub source_type: DataSourceType,
    pub connection_info: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceType {
    Snowflake,
    Dbt,
    BigQuery,
    Redshift,
    Postgres,
}

/// A database schema within a data source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub id: Uuid,
    pub data_source_id: Uuid,
    pub database_name: String,
    pub schema_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A table or view within a schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub id: Uuid,
    pub schema_id: Uuid,
    pub name: String,
    pub table_type: TableType,
    /// Human-readable description (from dbt, Snowflake comment, or inferred).
    pub description: Option<String>,
    /// dbt model name, if applicable.
    pub dbt_model: Option<String>,
    /// Owner/team responsible.
    pub owner: Option<String>,
    /// Row count (from profiling or information_schema).
    pub row_count: Option<i64>,
    /// Approximate byte size.
    pub byte_size: Option<i64>,
    /// Confidence score (0.0–1.0) on the metadata accuracy.
    pub confidence: f64,
    /// When the confidence was last refreshed.
    pub confidence_refreshed_at: Option<DateTime<Utc>>,
    /// Tags (arbitrary key-value pairs stored as JSON).
    pub tags: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TableType {
    BaseTable,
    View,
    MaterializedView,
    ExternalTable,
}
