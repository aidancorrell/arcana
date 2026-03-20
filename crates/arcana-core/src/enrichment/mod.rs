pub mod claude;

use anyhow::Result;
use async_trait::async_trait;

/// A request to generate a semantic definition for a data entity.
#[derive(Debug, Clone)]
pub struct EnrichmentRequest {
    /// Table name (e.g. "fct_orders")
    pub table_name: String,
    /// Column names in the table
    pub column_names: Vec<String>,
    /// Optional: names of upstream tables from lineage
    pub upstream_tables: Vec<String>,
    /// Optional: name of the specific column to describe (None = describe the table)
    pub column_name: Option<String>,
}

/// Response from the enrichment provider — one generated definition per request.
#[derive(Debug, Clone)]
pub struct EnrichmentResponse {
    pub definition: String,
}

/// Generates semantic definitions for data entities using an LLM.
#[async_trait]
pub trait EnrichmentProvider: Send + Sync {
    /// Generate definitions for a batch of requests in a single LLM call.
    async fn enrich_batch(
        &self,
        requests: &[EnrichmentRequest],
    ) -> Result<Vec<EnrichmentResponse>>;
}
