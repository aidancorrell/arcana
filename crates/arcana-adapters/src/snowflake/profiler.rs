use anyhow::Result;
use arcana_core::entities::ColumnProfile;
use uuid::Uuid;

use super::SnowflakeConfig;

/// Profile a column by running statistics queries against Snowflake.
pub async fn profile_column(
    config: &SnowflakeConfig,
    column_id: Uuid,
    table_fqn: &str,
    column_name: &str,
) -> Result<ColumnProfile> {
    let _ = (config, column_id, table_fqn, column_name);
    // TODO: execute profiling SQL:
    //   SELECT
    //     COUNT(*) - COUNT(<col>) AS null_count,
    //     (COUNT(*) - COUNT(<col>)) / COUNT(*) AS null_pct,
    //     COUNT(DISTINCT <col>) AS distinct_count,
    //     MIN(<col>), MAX(<col>), AVG(TRY_TO_NUMBER(<col>)), STDDEV(TRY_TO_NUMBER(<col>))
    //   FROM <table_fqn>
    todo!("implement Snowflake column profiling")
}
