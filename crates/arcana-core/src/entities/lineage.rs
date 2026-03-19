use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A directed edge in the data lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEdge {
    pub id: Uuid,
    /// The upstream entity (table or column ID).
    pub upstream_id: Uuid,
    pub upstream_type: LineageNodeType,
    /// The downstream entity (table or column ID).
    pub downstream_id: Uuid,
    pub downstream_type: LineageNodeType,
    /// How this edge was discovered.
    pub source: LineageSource,
    /// Optional SQL or transformation expression explaining the relationship.
    pub transform_expression: Option<String>,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineageNodeType {
    Table,
    Column,
    Metric,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineageSource {
    /// Parsed from dbt manifest.json node references.
    DbtManifest,
    /// Inferred from Snowflake query history.
    SnowflakeQueryHistory,
    /// Manually specified.
    Manual,
    /// Inferred by an LLM from SQL.
    LlmInferred,
}
