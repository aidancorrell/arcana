use anyhow::Result;
use arcana_core::store::MetadataStore;
use arcana_recommender::ranker::{ContextRequest, RelevanceRanker};
use arcana_recommender::serializer::ContextSerializer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tool input/output types
// ---------------------------------------------------------------------------

/// Input for the `get_context` tool.
#[derive(Debug, Deserialize)]
pub struct GetContextInput {
    /// Natural-language query describing what context the agent needs.
    pub query: String,
    /// Maximum number of results (default: 10).
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Minimum confidence score to include (default: 0.0).
    #[serde(default)]
    pub min_confidence: f64,
}

fn default_top_k() -> usize {
    10
}

#[derive(Debug, Serialize)]
pub struct GetContextOutput {
    pub context: String,
    pub item_count: usize,
    pub estimated_tokens: usize,
}

/// Input for the `describe_table` tool.
#[derive(Debug, Deserialize)]
pub struct DescribeTableInput {
    /// Fully-qualified table name (e.g., `ANALYTICS.PUBLIC.ORDERS`) or UUID.
    pub table_ref: String,
    /// Whether to include column-level detail.
    #[serde(default = "default_true")]
    pub include_columns: bool,
    /// Whether to include semantic definitions.
    #[serde(default = "default_true")]
    pub include_definitions: bool,
    /// Whether to include data contracts.
    #[serde(default)]
    pub include_contracts: bool,
    /// Whether to include lineage.
    #[serde(default)]
    pub include_lineage: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct DescribeTableOutput {
    pub description: String,
    pub table_id: Option<Uuid>,
    pub confidence: f64,
}

/// Input for the `estimate_cost` tool.
#[derive(Debug, Deserialize)]
pub struct EstimateCostInput {
    /// SQL query to estimate cost for.
    pub sql: String,
    /// Target warehouse size (e.g., `"SMALL"`, `"MEDIUM"`).
    #[serde(default = "default_warehouse_size")]
    pub warehouse_size: String,
}

fn default_warehouse_size() -> String {
    "SMALL".to_string()
}

#[derive(Debug, Serialize)]
pub struct EstimateCostOutput {
    pub estimated_credits: f64,
    pub estimated_usd: f64,
    pub explanation: String,
}

/// Input for the `update_context` tool.
///
/// Allows an agent to push new semantic context back into Arcana (human-in-the-loop).
#[derive(Debug, Deserialize)]
pub struct UpdateContextInput {
    /// The entity being annotated.
    pub entity_id: Uuid,
    pub entity_type: String,
    /// The new semantic definition.
    pub definition: String,
    /// Confidence the agent assigns to this definition.
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    0.8
}

#[derive(Debug, Serialize)]
pub struct UpdateContextOutput {
    pub success: bool,
    pub definition_id: Uuid,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Tool handler implementations
// ---------------------------------------------------------------------------

/// Handles the `get_context` MCP tool.
pub async fn handle_get_context(
    input: GetContextInput,
    ranker: Arc<RelevanceRanker>,
    serializer: Arc<ContextSerializer>,
) -> Result<GetContextOutput> {
    let request = ContextRequest {
        query: input.query,
        top_k: input.top_k,
        min_confidence: input.min_confidence,
        filter_table_id: None,
    };

    let result = ranker.rank(&request).await?;
    let item_count = result.items.len();
    let estimated_tokens = result.estimated_tokens;
    let context = serializer.serialize(&result);

    Ok(GetContextOutput {
        context,
        item_count,
        estimated_tokens,
    })
}

/// Handles the `describe_table` MCP tool.
pub async fn handle_describe_table(
    input: DescribeTableInput,
    store: Arc<dyn MetadataStore>,
) -> Result<DescribeTableOutput> {
    // Try to parse as UUID first, otherwise search by name.
    let table = if let Ok(id) = input.table_ref.parse::<Uuid>() {
        store.get_table(id).await?
    } else {
        store
            .search_tables(&input.table_ref, 1)
            .await?
            .into_iter()
            .next()
    };

    match table {
        None => Ok(DescribeTableOutput {
            description: format!("Table '{}' not found in Arcana.", input.table_ref),
            table_id: None,
            confidence: 0.0,
        }),
        Some(table) => {
            let columns = if input.include_columns {
                store.list_columns(table.id).await?
            } else {
                vec![]
            };

            let definitions = if input.include_definitions {
                store.get_semantic_definitions(table.id).await?
            } else {
                vec![]
            };

            let description =
                ContextSerializer::format_table(&table, &columns, &definitions);

            Ok(DescribeTableOutput {
                confidence: table.confidence,
                table_id: Some(table.id),
                description,
            })
        }
    }
}

/// Handles the `estimate_cost` MCP tool.
pub async fn handle_estimate_cost(
    input: EstimateCostInput,
    snowflake_config: Option<Arc<arcana_adapters::snowflake::SnowflakeConfig>>,
) -> Result<EstimateCostOutput> {
    let config = snowflake_config
        .ok_or_else(|| anyhow::anyhow!("Snowflake is not configured — cannot estimate cost"))?;

    let mut client =
        arcana_adapters::snowflake::client::SnowflakeClient::new((*config).clone());
    let estimate =
        arcana_adapters::snowflake::cost::estimate_query_cost(&mut client, &input.sql, &input.warehouse_size)
            .await?;

    Ok(EstimateCostOutput {
        estimated_credits: estimate.credits,
        estimated_usd: estimate.estimated_usd,
        explanation: estimate.explanation,
    })
}

/// Handles the `update_context` MCP tool.
pub async fn handle_update_context(
    input: UpdateContextInput,
    store: Arc<dyn MetadataStore>,
) -> Result<UpdateContextOutput> {
    use arcana_core::entities::{DefinitionSource, SemanticDefinition, SemanticEntityType};

    let entity_type = match input.entity_type.as_str() {
        "table" => SemanticEntityType::Table,
        "column" => SemanticEntityType::Column,
        "metric" => SemanticEntityType::Metric,
        other => anyhow::bail!("unknown entity_type: {other}"),
    };

    let now = chrono::Utc::now();
    let def = SemanticDefinition {
        id: Uuid::new_v4(),
        entity_id: input.entity_id,
        entity_type,
        definition: input.definition,
        source: DefinitionSource::LlmInferred,
        confidence: input.confidence,
        confidence_refreshed_at: Some(now),
        embedding: None,
        created_at: now,
        updated_at: now,
    };

    store.upsert_semantic_definition(&def).await?;

    Ok(UpdateContextOutput {
        success: true,
        definition_id: def.id,
        message: format!(
            "Semantic definition {} created for entity {}.",
            def.id, input.entity_id
        ),
    })
}
