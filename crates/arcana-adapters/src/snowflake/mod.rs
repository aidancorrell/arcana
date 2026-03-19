pub mod cost;
pub mod profiler;
pub mod schema_sync;
pub mod usage;

use anyhow::Result;
use async_trait::async_trait;

use crate::adapter::{MetadataAdapter, SyncOutput};

/// Configuration for the Snowflake adapter.
#[derive(Debug, Clone)]
pub struct SnowflakeConfig {
    pub account: String,
    pub warehouse: String,
    pub database: String,
    pub schema: String,
    pub user: String,
    /// Prefer private_key_path; fall back to password.
    pub private_key_path: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
}

/// Snowflake metadata adapter.
pub struct SnowflakeAdapter {
    config: SnowflakeConfig,
}

impl SnowflakeAdapter {
    pub fn new(config: SnowflakeConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl MetadataAdapter for SnowflakeAdapter {
    fn name(&self) -> &str {
        "snowflake"
    }

    async fn sync(&self) -> Result<SyncOutput> {
        let output = schema_sync::sync_schemas(&self.config).await?;
        Ok(output)
    }

    async fn health_check(&self) -> Result<()> {
        // TODO: execute `SELECT CURRENT_TIMESTAMP()` via Snowflake HTTP API
        todo!("implement Snowflake health check")
    }
}
