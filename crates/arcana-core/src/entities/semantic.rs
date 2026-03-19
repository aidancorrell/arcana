use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A semantic definition attached to a table or column — the "what does this mean" layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDefinition {
    pub id: Uuid,
    /// The entity this definition is about (table or column ID).
    pub entity_id: Uuid,
    pub entity_type: SemanticEntityType,
    /// Human-readable definition text.
    pub definition: String,
    /// Where this definition came from.
    pub source: DefinitionSource,
    /// Confidence in the accuracy of this definition (0.0–1.0).
    pub confidence: f64,
    pub confidence_refreshed_at: Option<DateTime<Utc>>,
    /// Embedding vector (stored as JSON array for SQLite compat; use usearch for ANN).
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticEntityType {
    Table,
    Column,
    Metric,
}

/// Provenance of a semantic definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionSource {
    /// Explicitly written in a dbt schema YAML.
    DbtYaml,
    /// Extracted from a document (wiki, Confluence, Markdown).
    Document,
    /// Inferred by an LLM from schema + samples.
    LlmInferred,
    /// Manually entered via CLI or UI.
    Manual,
    /// Pulled from Snowflake column/table comments.
    SnowflakeComment,
}

/// A business metric definition (e.g., from dbt Semantic Layer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub id: Uuid,
    pub name: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub metric_type: MetricType,
    /// The dbt model or table this metric is computed from.
    pub source_table_id: Option<Uuid>,
    /// Raw SQL or dbt metric expression.
    pub expression: Option<String>,
    pub dimensions: Vec<String>,
    pub filters: Option<serde_json::Value>,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    Simple,
    Ratio,
    Cumulative,
    Derived,
}
