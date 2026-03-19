use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A data contract asserting expectations on a table or column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataContract {
    pub id: Uuid,
    pub name: String,
    pub entity_id: Uuid,
    pub entity_type: ContractEntityType,
    pub contract_type: ContractType,
    /// Human-readable description of what this contract asserts.
    pub description: Option<String>,
    /// The actual contract expression (SQL, YAML, JSON Schema, etc.).
    pub expression: serde_json::Value,
    pub status: ContractStatus,
    /// When this contract was last evaluated.
    pub last_evaluated_at: Option<DateTime<Utc>>,
    /// Result of the last evaluation.
    pub last_result: Option<ContractResult>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContractEntityType {
    Table,
    Column,
    Dataset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContractType {
    /// Row-count assertion (min/max bounds).
    RowCount,
    /// Column nullability assertion.
    NotNull,
    /// Uniqueness assertion.
    Unique,
    /// Referential integrity assertion.
    ReferentialIntegrity,
    /// Custom SQL assertion.
    CustomSql,
    /// JSON Schema validation.
    JsonSchema,
    /// dbt test (maps from dbt manifest).
    DbtTest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Active,
    Disabled,
    Draft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContractResult {
    Pass,
    Fail,
    Error,
    Skipped,
}
