//! Integration tests for MCP tool handlers: get_context, describe_table,
//! update_context, and estimate_cost.

use anyhow::Result;
use arcana_core::{
    embeddings::{EmbeddingProvider, VectorIndex},
    entities::*,
    store::{MetadataStore, SqliteStore},
};
use arcana_mcp::tools::{
    DescribeTableInput, GetContextInput, UpdateContextInput,
    handle_describe_table, handle_get_context, handle_update_context,
};
use arcana_recommender::{
    ranker::RelevanceRanker,
    serializer::{ContextSerializer, SerializationFormat},
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Mock embedding provider
// ---------------------------------------------------------------------------

struct MockEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vec = vec![0.0f32; 16];
        for (i, byte) in text.bytes().enumerate() {
            vec[i % 16] += (byte as f32) / 255.0;
        }
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        Ok(vec)
    }

    fn dimensions(&self) -> usize {
        16
    }

    fn name(&self) -> &str {
        "mock"
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

async fn setup() -> (Arc<SqliteStore>, Arc<RelevanceRanker>, Arc<ContextSerializer>) {
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());
    let provider = Arc::new(MockEmbeddingProvider);
    let entity_index = Arc::new(VectorIndex::new(16));
    let chunk_index = Arc::new(VectorIndex::new(16));
    let ranker = Arc::new(RelevanceRanker::new(
        store.clone(),
        provider,
        entity_index,
        chunk_index,
    ));
    let serializer = Arc::new(ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::Markdown,
    });
    (store, ranker, serializer)
}

async fn seed_full_table(store: &SqliteStore) -> Table {
    let ds = DataSource {
        id: Uuid::new_v4(),
        name: "test".into(),
        source_type: DataSourceType::Snowflake,
        connection_info: serde_json::json!({}),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_data_source(&ds).await.unwrap();

    let schema = Schema {
        id: Uuid::new_v4(),
        data_source_id: ds.id,
        database_name: "ANALYTICS".into(),
        schema_name: "PUBLIC".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_schema(&schema).await.unwrap();

    let table = Table {
        id: Uuid::new_v4(),
        schema_id: schema.id,
        name: "fct_orders".into(),
        table_type: TableType::BaseTable,
        description: Some("One row per order".into()),
        dbt_model: Some("fct_orders".into()),
        owner: Some("data-eng".into()),
        row_count: Some(5_000_000),
        byte_size: None,
        confidence: 0.9,
        confidence_refreshed_at: Some(now()),
        tags: serde_json::json!({}),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_table(&table).await.unwrap();

    // Add columns
    for (i, (name, dtype)) in [("order_id", "INT"), ("amount", "DECIMAL(12,2)"), ("status", "VARCHAR")]
        .iter()
        .enumerate()
    {
        let col = Column {
            id: Uuid::new_v4(),
            table_id: table.id,
            name: name.to_string(),
            data_type: dtype.to_string(),
            ordinal_position: i as i32,
            is_nullable: i > 0,
            is_primary_key: i == 0,
            is_foreign_key: false,
            description: Some(format!("The {name} column")),
            dbt_meta: None,
            tags: serde_json::json!({}),
            confidence: 0.85,
            confidence_refreshed_at: Some(now()),
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_column(&col).await.unwrap();
    }

    // Add semantic definition
    let def = SemanticDefinition {
        id: Uuid::new_v4(),
        entity_id: table.id,
        entity_type: SemanticEntityType::Table,
        definition: "Contains one row per customer order. Grain: order_id.".into(),
        source: DefinitionSource::DbtYaml,
        confidence: 0.85,
        confidence_refreshed_at: Some(now()),
        embedding: None,
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_semantic_definition(&def).await.unwrap();

    table
}

// ---------------------------------------------------------------------------
// get_context tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_context_empty_index() {
    let (_, ranker, serializer) = setup().await;

    let input = GetContextInput {
        query: "monthly revenue".into(),
        top_k: 10,
        min_confidence: 0.0,
    };

    let output = handle_get_context(input, ranker, serializer).await.unwrap();
    assert_eq!(output.item_count, 0);
    assert!(output.context.contains("No relevant context"));
}

#[tokio::test]
async fn get_context_with_min_confidence_filter() {
    let (_, ranker, serializer) = setup().await;

    let input = GetContextInput {
        query: "anything".into(),
        top_k: 10,
        min_confidence: 0.99, // Very high threshold
    };

    let output = handle_get_context(input, ranker, serializer).await.unwrap();
    assert_eq!(output.item_count, 0);
}

// ---------------------------------------------------------------------------
// describe_table tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn describe_table_by_name() {
    let (store, _, _) = setup().await;
    let table = seed_full_table(&store).await;

    let input = DescribeTableInput {
        table_ref: "fct_orders".into(),
        include_columns: true,
        include_definitions: true,
        include_contracts: false,
        include_lineage: false,
    };

    let output = handle_describe_table(input, store).await.unwrap();
    assert!(output.table_id.is_some());
    assert_eq!(output.table_id.unwrap(), table.id);
    assert_eq!(output.confidence, 0.9);

    // Description should include table info
    assert!(output.description.contains("fct_orders"));
    assert!(output.description.contains("One row per order"));
    assert!(output.description.contains("data-eng"));
    assert!(output.description.contains("5000000"));

    // Should include columns
    assert!(output.description.contains("order_id"));
    assert!(output.description.contains("amount"));
    assert!(output.description.contains("status"));

    // Should include semantic definitions
    assert!(output.description.contains("Contains one row per customer order"));
}

#[tokio::test]
async fn describe_table_by_uuid() {
    let (store, _, _) = setup().await;
    let table = seed_full_table(&store).await;

    let input = DescribeTableInput {
        table_ref: table.id.to_string(),
        include_columns: false,
        include_definitions: false,
        include_contracts: false,
        include_lineage: false,
    };

    let output = handle_describe_table(input, store).await.unwrap();
    assert_eq!(output.table_id.unwrap(), table.id);
    // Should not include columns or definitions
    assert!(!output.description.contains("| `order_id`"));
}

#[tokio::test]
async fn describe_table_not_found() {
    let (store, _, _) = setup().await;

    let input = DescribeTableInput {
        table_ref: "nonexistent_table".into(),
        include_columns: true,
        include_definitions: true,
        include_contracts: false,
        include_lineage: false,
    };

    let output = handle_describe_table(input, store).await.unwrap();
    assert!(output.table_id.is_none());
    assert_eq!(output.confidence, 0.0);
    assert!(output.description.contains("not found"));
}

#[tokio::test]
async fn describe_table_without_columns() {
    let (store, _, _) = setup().await;
    seed_full_table(&store).await;

    let input = DescribeTableInput {
        table_ref: "fct_orders".into(),
        include_columns: false,
        include_definitions: true,
        include_contracts: false,
        include_lineage: false,
    };

    let output = handle_describe_table(input, store).await.unwrap();
    assert!(output.description.contains("fct_orders"));
    // Columns table header should not appear
    assert!(!output.description.contains("| Name | Type |"));
}

// ---------------------------------------------------------------------------
// update_context tool tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_context_creates_definition() {
    let (store, _, _) = setup().await;
    let entity_id = Uuid::new_v4();

    let input = UpdateContextInput {
        entity_id,
        entity_type: "table".into(),
        definition: "This table tracks all monthly billing events.".into(),
        confidence: 0.75,
    };

    let output = handle_update_context(input, store.clone()).await.unwrap();
    assert!(output.success);

    // Verify it was persisted
    let defs = store.get_semantic_definitions(entity_id).await.unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(
        defs[0].definition,
        "This table tracks all monthly billing events."
    );
    assert_eq!(defs[0].source, DefinitionSource::LlmInferred);
    assert_eq!(defs[0].confidence, 0.75);
}

#[tokio::test]
async fn update_context_column_type() {
    let (store, _, _) = setup().await;
    let entity_id = Uuid::new_v4();

    let input = UpdateContextInput {
        entity_id,
        entity_type: "column".into(),
        definition: "Revenue in USD".into(),
        confidence: 0.8,
    };

    let output = handle_update_context(input, store.clone()).await.unwrap();
    assert!(output.success);

    let defs = store.get_semantic_definitions(entity_id).await.unwrap();
    assert_eq!(defs[0].entity_type, SemanticEntityType::Column);
}

#[tokio::test]
async fn update_context_metric_type() {
    let (store, _, _) = setup().await;
    let entity_id = Uuid::new_v4();

    let input = UpdateContextInput {
        entity_id,
        entity_type: "metric".into(),
        definition: "Monthly active users".into(),
        confidence: 0.6,
    };

    let output = handle_update_context(input, store.clone()).await.unwrap();
    assert!(output.success);

    let defs = store.get_semantic_definitions(entity_id).await.unwrap();
    assert_eq!(defs[0].entity_type, SemanticEntityType::Metric);
}

#[tokio::test]
async fn update_context_invalid_entity_type() {
    let (store, _, _) = setup().await;

    let input = UpdateContextInput {
        entity_id: Uuid::new_v4(),
        entity_type: "unknown".into(),
        definition: "some definition".into(),
        confidence: 0.8,
    };

    let result = handle_update_context(input, store).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown entity_type"));
}

// ---------------------------------------------------------------------------
// estimate_cost tool tests (without Snowflake config)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn estimate_cost_no_snowflake_config() {
    use arcana_mcp::tools::{EstimateCostInput, handle_estimate_cost};

    let input = EstimateCostInput {
        sql: "SELECT * FROM orders".into(),
        warehouse_size: "SMALL".into(),
    };

    let result = handle_estimate_cost(input, None).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Snowflake is not configured"));
}

// ---------------------------------------------------------------------------
// MCP server info tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_info_is_correct() {
    let (store, ranker, serializer) = setup().await;

    let server = arcana_mcp::ArcanaServer::new(store, ranker, serializer);

    use rmcp::ServerHandler;

    let info = server.get_info();
    assert_eq!(info.server_info.name.as_str(), "arcana");
    assert!(info.instructions.is_some());
    assert!(info.capabilities.tools.is_some());
}
