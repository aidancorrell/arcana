use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use arcana_core::entities::{QueryType, UsageRecord};

use super::client::{column_index, get_cell, SnowflakeClient};

/// Pull query history from Snowflake ACCOUNT_USAGE.QUERY_HISTORY and map to UsageRecords.
///
/// Note: ACCOUNT_USAGE views have a 45-minute latency and require the
/// SNOWFLAKE database usage privilege.
pub async fn pull_usage_records(
    client: &mut SnowflakeClient,
    lookback_hours: u32,
) -> Result<Vec<UsageRecord>> {
    let sql = format!(
        "SELECT \
            QUERY_ID, \
            QUERY_TYPE, \
            USER_NAME, \
            WAREHOUSE_NAME, \
            BYTES_SCANNED, \
            CREDITS_USED_CLOUD_SERVICES, \
            TOTAL_ELAPSED_TIME, \
            START_TIME \
         FROM SNOWFLAKE.ACCOUNT_USAGE.QUERY_HISTORY \
         WHERE START_TIME >= DATEADD(hour, -{}, CURRENT_TIMESTAMP()) \
           AND EXECUTION_STATUS = 'SUCCESS' \
         ORDER BY START_TIME DESC \
         LIMIT 10000",
        lookback_hours
    );

    let resp = client
        .execute_sql(&sql)
        .await
        .context("failed to query SNOWFLAKE.ACCOUNT_USAGE.QUERY_HISTORY")?;

    let meta = &resp.result_set_metadata;
    let query_type_idx = column_index(meta, "QUERY_TYPE").unwrap_or(1);
    let user_idx = column_index(meta, "USER_NAME").unwrap_or(2);
    let warehouse_idx = column_index(meta, "WAREHOUSE_NAME").unwrap_or(3);
    let bytes_idx = column_index(meta, "BYTES_SCANNED").unwrap_or(4);
    let credits_idx = column_index(meta, "CREDITS_USED_CLOUD_SERVICES").unwrap_or(5);
    let duration_idx = column_index(meta, "TOTAL_ELAPSED_TIME").unwrap_or(6);
    let start_time_idx = column_index(meta, "START_TIME").unwrap_or(7);

    let mut records = Vec::new();

    for row in &resp.data {
        let query_type = parse_query_type(get_cell(row, query_type_idx).unwrap_or(""));
        let actor = get_cell(row, user_idx).map(|s| s.to_string());
        let warehouse = get_cell(row, warehouse_idx).map(|s| s.to_string());
        let bytes_scanned = get_cell(row, bytes_idx).and_then(|s| s.parse::<i64>().ok());
        let credits_used = get_cell(row, credits_idx).and_then(|s| s.parse::<f64>().ok());
        let duration_ms = get_cell(row, duration_idx).and_then(|s| s.parse::<i64>().ok());
        let executed_at = get_cell(row, start_time_idx)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        records.push(UsageRecord {
            id: Uuid::new_v4(),
            table_id: Uuid::nil(), // resolved by caller via query text parsing
            actor,
            warehouse,
            query_type,
            bytes_scanned,
            credits_used,
            duration_ms,
            executed_at,
            created_at: Utc::now(),
        });
    }

    tracing::info!(
        "pulled {} usage records from Snowflake (last {} hours)",
        records.len(),
        lookback_hours,
    );

    Ok(records)
}

fn parse_query_type(s: &str) -> QueryType {
    match s.to_uppercase().as_str() {
        "SELECT" => QueryType::Select,
        "INSERT" => QueryType::Insert,
        "UPDATE" => QueryType::Update,
        "DELETE" | "TRUNCATE" | "TRUNCATE_TABLE" => QueryType::Delete,
        "MERGE" => QueryType::Merge,
        "CREATE" | "CREATE_TABLE" | "CREATE_TABLE_AS_SELECT" => QueryType::Create,
        "DROP" | "DROP_TABLE" => QueryType::Drop,
        _ => QueryType::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_type() {
        assert_eq!(parse_query_type("SELECT"), QueryType::Select);
        assert_eq!(parse_query_type("INSERT"), QueryType::Insert);
        assert_eq!(parse_query_type("MERGE"), QueryType::Merge);
        assert_eq!(parse_query_type("CREATE_TABLE_AS_SELECT"), QueryType::Create);
        assert_eq!(parse_query_type("TRUNCATE_TABLE"), QueryType::Delete);
        assert_eq!(parse_query_type("UNKNOWN_TYPE"), QueryType::Other);
    }
}
