use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A group of semantically similar tables, with one designated as canonical.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableCluster {
    pub id: Uuid,
    pub label: Option<String>,
    pub canonical_id: Option<Uuid>,
    pub threshold: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Membership of a table in a cluster, with its similarity to the canonical.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableClusterMember {
    pub cluster_id: Uuid,
    pub table_id: Uuid,
    pub similarity: f64,
}
