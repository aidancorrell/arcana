use anyhow::Result;
use arcana_core::entities::UsageRecord;

use super::SnowflakeConfig;

/// Pull query history from Snowflake ACCOUNT_USAGE.QUERY_HISTORY and map to UsageRecords.
pub async fn pull_usage_records(
    config: &SnowflakeConfig,
    lookback_hours: u32,
) -> Result<Vec<UsageRecord>> {
    let _ = (config, lookback_hours);
    // TODO: query SNOWFLAKE.ACCOUNT_USAGE.QUERY_HISTORY
    //   (requires ACCOUNT_USAGE privilege, 45-minute latency)
    todo!("implement Snowflake usage record pull")
}
