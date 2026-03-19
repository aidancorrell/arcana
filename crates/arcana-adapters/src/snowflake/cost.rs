use anyhow::Result;

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

/// Estimate cost of running a SQL query against a given table.
pub async fn estimate_query_cost(
    table_fqn: &str,
    query_sql: &str,
    warehouse_size: &str,
) -> Result<CostEstimate> {
    let _ = (table_fqn, query_sql, warehouse_size);
    // TODO: use EXPLAIN in Snowflake to get bytes scanned estimate,
    //       then apply warehouse credit rate table.
    todo!("implement Snowflake cost estimation")
}
