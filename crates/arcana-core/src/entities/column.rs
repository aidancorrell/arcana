use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A column within a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub id: Uuid,
    pub table_id: Uuid,
    pub name: String,
    pub data_type: String,
    pub ordinal_position: i32,
    pub is_nullable: bool,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
    pub description: Option<String>,
    /// dbt column-level description/meta.
    pub dbt_meta: Option<serde_json::Value>,
    /// Tags (arbitrary key-value pairs stored as JSON).
    pub tags: serde_json::Value,
    pub confidence: f64,
    pub confidence_refreshed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Statistical profile of a column's data distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnProfile {
    pub id: Uuid,
    pub column_id: Uuid,
    pub null_count: i64,
    pub null_pct: f64,
    pub distinct_count: Option<i64>,
    pub min_value: Option<serde_json::Value>,
    pub max_value: Option<serde_json::Value>,
    pub mean_value: Option<f64>,
    pub stddev_value: Option<f64>,
    /// Most frequent values (top-N).
    pub top_values: Option<serde_json::Value>,
    pub profiled_at: DateTime<Utc>,
}
