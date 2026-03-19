use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A record of a query or operation that touched a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Uuid,
    pub table_id: Uuid,
    /// The user or service account that ran the query.
    pub actor: Option<String>,
    /// Warehouse/compute used (Snowflake-specific).
    pub warehouse: Option<String>,
    pub query_type: QueryType,
    /// Bytes scanned (for cost estimation).
    pub bytes_scanned: Option<i64>,
    /// Credits consumed (Snowflake-specific).
    pub credits_used: Option<f64>,
    /// Duration in milliseconds.
    pub duration_ms: Option<i64>,
    pub executed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueryType {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
    Create,
    Drop,
    Other,
}

/// A record of an AI agent interacting with Arcana's MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInteraction {
    pub id: Uuid,
    /// The MCP tool that was called.
    pub tool_name: String,
    /// The input provided to the tool (sanitized).
    pub input: serde_json::Value,
    /// Entity IDs that were returned/referenced.
    pub referenced_entity_ids: Vec<Uuid>,
    /// Agent identifier (from MCP client metadata).
    pub agent_id: Option<String>,
    /// Whether the agent marked this context as helpful (thumbs-up signal).
    pub was_helpful: Option<bool>,
    /// Latency of the tool call in milliseconds.
    pub latency_ms: Option<i64>,
    pub created_at: DateTime<Utc>,
}
