use anyhow::Result;

use super::SnowflakeConfig;
use crate::adapter::SyncOutput;

/// Sync schemas, tables, and columns from Snowflake INFORMATION_SCHEMA.
///
/// Uses the Snowflake SQL API (REST) to execute queries and parse results.
pub async fn sync_schemas(config: &SnowflakeConfig) -> Result<SyncOutput> {
    let _ = config;
    // TODO: implement Snowflake SQL API calls:
    //   1. Query INFORMATION_SCHEMA.TABLES
    //   2. Query INFORMATION_SCHEMA.COLUMNS
    //   3. Query INFORMATION_SCHEMA.TABLE_CONSTRAINTS (PKs/FKs)
    //   4. Map rows to arcana-core entity types
    //   5. Return populated SyncOutput
    todo!("implement Snowflake schema sync via SQL API")
}
