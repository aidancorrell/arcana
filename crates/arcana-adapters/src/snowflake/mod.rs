pub mod client;
pub mod cost;
pub mod profiler;
pub mod schema_sync;
pub mod usage;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

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

impl SnowflakeConfig {
    /// Validate that all identifier fields are safe for use in SQL statements.
    /// Snowflake identifiers must match `[a-zA-Z0-9_.-]` to prevent SQL injection.
    pub fn validate(&self) -> Result<()> {
        validate_identifier(&self.database, "database")?;
        validate_identifier(&self.schema, "schema")?;
        validate_identifier(&self.warehouse, "warehouse")?;
        validate_identifier(&self.account, "account")?;
        validate_identifier(&self.user, "user")?;
        Ok(())
    }
}

/// Validate a Snowflake identifier against allowed characters.
fn validate_identifier(value: &str, field_name: &str) -> Result<()> {
    if value.is_empty() {
        anyhow::bail!("Snowflake {field_name} cannot be empty");
    }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-') {
        anyhow::bail!(
            "Invalid Snowflake {field_name}: '{}' — must contain only [a-zA-Z0-9_.-]",
            value
        );
    }
    Ok(())
}

/// Snowflake metadata adapter.
pub struct SnowflakeAdapter {
    config: SnowflakeConfig,
    data_source_id: Uuid,
}

impl SnowflakeAdapter {
    pub fn new(config: SnowflakeConfig, data_source_id: Uuid) -> Self {
        Self {
            config,
            data_source_id,
        }
    }

    pub fn config(&self) -> &SnowflakeConfig {
        &self.config
    }
}

#[async_trait]
impl MetadataAdapter for SnowflakeAdapter {
    fn name(&self) -> &str {
        "snowflake"
    }

    async fn sync(&self) -> Result<SyncOutput> {
        let output =
            schema_sync::sync_schemas(&self.config, self.data_source_id).await?;
        Ok(output)
    }

    async fn health_check(&self) -> Result<()> {
        let mut client = client::SnowflakeClient::new(self.config.clone());
        client.health_check().await
    }
}
