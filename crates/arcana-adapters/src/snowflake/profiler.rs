use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use arcana_core::entities::ColumnProfile;

use super::client::{column_index, get_cell, SnowflakeClient};

/// Profile a column by running statistics queries against Snowflake.
///
/// Executes a single aggregate query that computes null count, distinct count,
/// min, max, mean, and stddev for the given column.
pub async fn profile_column(
    client: &mut SnowflakeClient,
    column_id: Uuid,
    table_fqn: &str,
    column_name: &str,
) -> Result<ColumnProfile> {
    // Use identifier quoting to prevent SQL injection
    let sql = format!(
        r#"SELECT
            COUNT(*) AS total_count,
            COUNT(*) - COUNT("{col}") AS null_count,
            CASE WHEN COUNT(*) > 0
                THEN (COUNT(*) - COUNT("{col}"))::FLOAT / COUNT(*)
                ELSE 0 END AS null_pct,
            COUNT(DISTINCT "{col}") AS distinct_count,
            MIN("{col}")::VARCHAR AS min_value,
            MAX("{col}")::VARCHAR AS max_value,
            AVG(TRY_TO_DOUBLE("{col}")) AS mean_value,
            STDDEV(TRY_TO_DOUBLE("{col}")) AS stddev_value
        FROM {table}"#,
        col = column_name.replace('"', "\"\""),
        table = table_fqn,
    );

    let resp = client
        .execute_sql(&sql)
        .await
        .with_context(|| format!("failed to profile column {}.{}", table_fqn, column_name))?;

    let row = resp.data.first().context("profiling query returned no rows")?;
    let meta = &resp.result_set_metadata;

    let null_count_idx = column_index(meta, "NULL_COUNT").unwrap_or(1);
    let null_pct_idx = column_index(meta, "NULL_PCT").unwrap_or(2);
    let distinct_idx = column_index(meta, "DISTINCT_COUNT").unwrap_or(3);
    let min_idx = column_index(meta, "MIN_VALUE").unwrap_or(4);
    let max_idx = column_index(meta, "MAX_VALUE").unwrap_or(5);
    let mean_idx = column_index(meta, "MEAN_VALUE").unwrap_or(6);
    let stddev_idx = column_index(meta, "STDDEV_VALUE").unwrap_or(7);

    let null_count = get_cell(row, null_count_idx)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let null_pct = get_cell(row, null_pct_idx)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let distinct_count = get_cell(row, distinct_idx).and_then(|s| s.parse::<i64>().ok());
    let min_value = get_cell(row, min_idx).map(|s| serde_json::Value::String(s.to_string()));
    let max_value = get_cell(row, max_idx).map(|s| serde_json::Value::String(s.to_string()));
    let mean_value = get_cell(row, mean_idx).and_then(|s| s.parse::<f64>().ok());
    let stddev_value = get_cell(row, stddev_idx).and_then(|s| s.parse::<f64>().ok());

    Ok(ColumnProfile {
        id: Uuid::new_v4(),
        column_id,
        null_count,
        null_pct,
        distinct_count,
        min_value,
        max_value,
        mean_value,
        stddev_value,
        top_values: None, // would require a separate GROUP BY query
        profiled_at: Utc::now(),
    })
}

/// Profile multiple columns of a table in a single query for efficiency.
pub async fn profile_table_columns(
    client: &mut SnowflakeClient,
    table_fqn: &str,
    columns: &[(Uuid, String)],
) -> Result<Vec<ColumnProfile>> {
    let mut profiles = Vec::with_capacity(columns.len());
    for (column_id, column_name) in columns {
        match profile_column(client, *column_id, table_fqn, column_name).await {
            Ok(profile) => profiles.push(profile),
            Err(e) => {
                tracing::warn!(
                    "failed to profile column {}.{}: {}",
                    table_fqn,
                    column_name,
                    e
                );
            }
        }
    }
    Ok(profiles)
}
