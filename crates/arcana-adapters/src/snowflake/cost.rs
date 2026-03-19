use anyhow::{Context, Result};

use super::client::SnowflakeClient;

/// Estimate the Snowflake compute cost for a given query.
///
/// Snowflake credits are consumed based on warehouse size and query duration.
/// This estimate is based on bytes scanned and typical warehouse credit rates.
#[derive(Debug, Clone)]
pub struct CostEstimate {
    /// Estimated Snowflake credits consumed.
    pub credits: f64,
    /// Estimated USD cost (based on list price; may differ from contracted rate).
    pub estimated_usd: f64,
    /// Bytes that would be scanned.
    pub bytes_scanned: u64,
    /// Explanation of how the estimate was derived.
    pub explanation: String,
}

/// Credit-per-second rates by warehouse size (Snowflake standard pricing).
fn credits_per_second(warehouse_size: &str) -> f64 {
    match warehouse_size.to_uppercase().as_str() {
        "X-SMALL" | "XSMALL" => 1.0 / 3600.0,
        "SMALL" => 2.0 / 3600.0,
        "MEDIUM" => 4.0 / 3600.0,
        "LARGE" => 8.0 / 3600.0,
        "X-LARGE" | "XLARGE" => 16.0 / 3600.0,
        "2X-LARGE" | "2XLARGE" => 32.0 / 3600.0,
        "3X-LARGE" | "3XLARGE" => 64.0 / 3600.0,
        "4X-LARGE" | "4XLARGE" => 128.0 / 3600.0,
        _ => 1.0 / 3600.0, // default to X-Small
    }
}

/// USD per Snowflake credit (list price, on-demand).
const USD_PER_CREDIT: f64 = 2.50;

/// Estimate cost of running a SQL query using Snowflake EXPLAIN.
///
/// Runs `EXPLAIN USING JSON <query>` to get the bytes scanned estimate,
/// then calculates credits based on warehouse size.
pub async fn estimate_query_cost(
    client: &mut SnowflakeClient,
    query_sql: &str,
    warehouse_size: &str,
) -> Result<CostEstimate> {
    // Use EXPLAIN to get the query plan with estimated bytes
    let explain_sql = format!("EXPLAIN USING JSON {}", query_sql);
    let resp = client
        .execute_sql(&explain_sql)
        .await
        .context("failed to run EXPLAIN on query")?;

    // The EXPLAIN JSON output is returned as a single row with one column
    let bytes_scanned = extract_bytes_scanned(&resp.data);

    // Estimate query time based on bytes scanned.
    // Rough heuristic: Snowflake scans ~200MB/s per credit-unit of warehouse.
    // X-Small warehouse (1 credit/hr) ≈ 200 MB/s throughput.
    let scan_rate_bytes_per_sec: f64 = 200_000_000.0; // 200 MB/s baseline for X-Small
    let warehouse_multiplier = credits_per_second(warehouse_size) / credits_per_second("X-SMALL");
    let effective_scan_rate = scan_rate_bytes_per_sec * warehouse_multiplier;
    let estimated_seconds = bytes_scanned as f64 / effective_scan_rate;

    // Minimum 1 second (Snowflake minimum billing)
    let billed_seconds = estimated_seconds.max(1.0);
    let credits = billed_seconds * credits_per_second(warehouse_size);
    let estimated_usd = credits * USD_PER_CREDIT;

    let bytes_scanned_mb = bytes_scanned as f64 / 1_048_576.0;
    let explanation = format!(
        "Estimated {:.1} MB scanned on {} warehouse. \
         ~{:.1}s execution → {:.4} credits (${:.4} at ${:.2}/credit list price).",
        bytes_scanned_mb, warehouse_size, billed_seconds, credits, estimated_usd, USD_PER_CREDIT
    );

    Ok(CostEstimate {
        credits,
        estimated_usd,
        bytes_scanned,
        explanation,
    })
}

/// Extract bytes scanned from EXPLAIN JSON output.
///
/// The EXPLAIN USING JSON output contains a `GlobalStats.bytesAssigned` or
/// `partitionsAssigned` field. We look for the `bytesAssigned` value.
fn extract_bytes_scanned(data: &[Vec<Option<String>>]) -> u64 {
    // EXPLAIN USING JSON returns a single cell with the JSON plan
    let json_str = data
        .first()
        .and_then(|row| row.first())
        .and_then(|cell| cell.as_deref())
        .unwrap_or("{}");

    let plan: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();

    // Try GlobalStats.bytesAssigned first
    if let Some(bytes) = plan
        .pointer("/GlobalStats/bytesAssigned")
        .and_then(|v| v.as_u64())
    {
        return bytes;
    }

    // Fallback: look for partitionsAssigned and estimate (128MB per partition)
    if let Some(partitions) = plan
        .pointer("/GlobalStats/partitionsAssigned")
        .and_then(|v| v.as_u64())
    {
        return partitions * 128 * 1024 * 1024; // 128MB per micro-partition
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credits_per_second() {
        assert!((credits_per_second("X-SMALL") - 1.0 / 3600.0).abs() < f64::EPSILON);
        assert!((credits_per_second("LARGE") - 8.0 / 3600.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_bytes_scanned_from_json() {
        let data = vec![vec![Some(
            r#"{"GlobalStats":{"bytesAssigned":1073741824}}"#.to_string(),
        )]];
        assert_eq!(extract_bytes_scanned(&data), 1_073_741_824);
    }

    #[test]
    fn test_extract_bytes_from_partitions_fallback() {
        let data = vec![vec![Some(
            r#"{"GlobalStats":{"partitionsAssigned":8}}"#.to_string(),
        )]];
        assert_eq!(extract_bytes_scanned(&data), 8 * 128 * 1024 * 1024);
    }

    #[test]
    fn test_extract_bytes_empty() {
        let data: Vec<Vec<Option<String>>> = vec![];
        assert_eq!(extract_bytes_scanned(&data), 0);
    }
}
