//! Integration tests for the recommender: ranker, serializer, and feedback recorder.

use anyhow::Result;
use arcana_core::{
    embeddings::{EmbeddingProvider, VectorIndex},
    entities::*,
    store::{MetadataStore, SqliteStore},
};
use arcana_recommender::{
    feedback::FeedbackRecorder,
    ranker::{ContextEntityType, ContextItem, ContextRequest, ContextResult, RelevanceRanker},
    serializer::{ContextSerializer, SerializationFormat},
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Mock embedding provider (deterministic, no external calls)
// ---------------------------------------------------------------------------

struct MockEmbeddingProvider {
    dims: usize,
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Generate a deterministic embedding based on text content.
        // Hash the text into a simple vector.
        let mut vec = vec![0.0f32; self.dims];
        for (i, byte) in text.bytes().enumerate() {
            vec[i % self.dims] += (byte as f32) / 255.0;
        }
        // Normalize
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        Ok(vec)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn name(&self) -> &str {
        "mock"
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

async fn setup() -> (Arc<SqliteStore>, Arc<RelevanceRanker>, Arc<VectorIndex>, Arc<VectorIndex>) {
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());
    let dims = 32;
    let provider = Arc::new(MockEmbeddingProvider { dims });
    let entity_index = Arc::new(VectorIndex::new(dims));
    let chunk_index = Arc::new(VectorIndex::new(dims));
    let ranker = Arc::new(RelevanceRanker::new(
        store.clone(),
        provider,
        entity_index.clone(),
        chunk_index.clone(),
    ));
    (store, ranker, entity_index, chunk_index)
}

/// Seed a table into the store and entity_index.
async fn seed_table_with_embedding(
    store: &SqliteStore,
    entity_index: &VectorIndex,
    provider: &dyn EmbeddingProvider,
    schema_id: Uuid,
    name: &str,
    confidence: f64,
) -> Table {
    let table = Table {
        id: Uuid::new_v4(),
        schema_id,
        name: name.into(),
        table_type: TableType::BaseTable,
        description: Some(format!("{name} table")),
        dbt_model: None,
        owner: None,
        row_count: None,
        byte_size: None,
        confidence,
        confidence_refreshed_at: Some(now()),
        tags: serde_json::json!({}),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_table(&table).await.unwrap();

    // Embed and index the table
    let embedding = provider.embed(name).await.unwrap();
    entity_index.upsert(table.id, embedding).unwrap();

    table
}

// ---------------------------------------------------------------------------
// RelevanceRanker tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ranker_returns_empty_for_no_indexed_entities() {
    let (_, ranker, _, _) = setup().await;
    let request = ContextRequest {
        query: "monthly revenue".into(),
        top_k: 10,
        filter_table_id: None,
        min_confidence: 0.0,
    };
    let result = ranker.rank(&request).await.unwrap();
    assert!(result.items.is_empty());
    assert_eq!(result.estimated_tokens, 0);
}

#[tokio::test]
async fn ranker_finds_relevant_tables() {
    let (store, ranker, entity_index, _) = setup().await;
    let provider = MockEmbeddingProvider { dims: 32 };

    // Seed prerequisites
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
        database_name: "db".into(),
        schema_name: "public".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_schema(&schema).await.unwrap();

    let _orders = seed_table_with_embedding(
        &store, &entity_index, &provider, schema.id, "orders", 0.9,
    )
    .await;
    let _customers = seed_table_with_embedding(
        &store, &entity_index, &provider, schema.id, "customers", 0.8,
    )
    .await;

    let request = ContextRequest {
        query: "orders".into(),
        top_k: 5,
        filter_table_id: None,
        min_confidence: 0.0,
    };
    let result = ranker.rank(&request).await.unwrap();
    assert!(!result.items.is_empty());

    // The most relevant item should be the orders table
    assert_eq!(result.items[0].entity_type, ContextEntityType::Table);
}

#[tokio::test]
async fn ranker_respects_top_k() {
    let (store, ranker, entity_index, _) = setup().await;
    let provider = MockEmbeddingProvider { dims: 32 };

    let ds = DataSource {
        id: Uuid::new_v4(),
        name: "test".into(),
        source_type: DataSourceType::Dbt,
        connection_info: serde_json::json!({}),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_data_source(&ds).await.unwrap();
    let schema = Schema {
        id: Uuid::new_v4(),
        data_source_id: ds.id,
        database_name: "db".into(),
        schema_name: "s".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_schema(&schema).await.unwrap();

    for i in 0..10 {
        seed_table_with_embedding(
            &store, &entity_index, &provider, schema.id, &format!("table_{i}"), 0.9,
        )
        .await;
    }

    let request = ContextRequest {
        query: "table".into(),
        top_k: 3,
        filter_table_id: None,
        min_confidence: 0.0,
    };
    let result = ranker.rank(&request).await.unwrap();
    assert!(result.items.len() <= 3);
}

#[tokio::test]
async fn ranker_deduplicates_by_entity_id() {
    let (store, ranker, entity_index, chunk_index) = setup().await;
    let provider = MockEmbeddingProvider { dims: 32 };

    let ds = DataSource {
        id: Uuid::new_v4(),
        name: "test".into(),
        source_type: DataSourceType::Dbt,
        connection_info: serde_json::json!({}),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_data_source(&ds).await.unwrap();
    let schema = Schema {
        id: Uuid::new_v4(),
        data_source_id: ds.id,
        database_name: "db".into(),
        schema_name: "s".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_schema(&schema).await.unwrap();

    let table = seed_table_with_embedding(
        &store, &entity_index, &provider, schema.id, "orders", 0.9,
    )
    .await;

    // Also add the same entity to chunk_index (simulating it appearing in both)
    let embedding = provider.embed("orders").await.unwrap();
    chunk_index.upsert(table.id, embedding).unwrap();

    let request = ContextRequest {
        query: "orders".into(),
        top_k: 10,
        filter_table_id: None,
        min_confidence: 0.0,
    };
    let result = ranker.rank(&request).await.unwrap();

    // Should not have duplicates
    let mut seen_ids: Vec<Uuid> = result.items.iter().map(|i| i.entity_id).collect();
    seen_ids.sort();
    seen_ids.dedup();
    assert_eq!(seen_ids.len(), result.items.len());
}

#[tokio::test]
async fn combined_score_formula() {
    // relevance * 0.7 + confidence * 0.3
    assert!((RelevanceRanker::combined_score(1.0, 1.0) - 1.0).abs() < 1e-9);
    assert!((RelevanceRanker::combined_score(0.0, 0.0) - 0.0).abs() < 1e-9);
    assert!((RelevanceRanker::combined_score(1.0, 0.0) - 0.7).abs() < 1e-9);
    assert!((RelevanceRanker::combined_score(0.0, 1.0) - 0.3).abs() < 1e-9);
    assert!((RelevanceRanker::combined_score(0.5, 0.5) - 0.5).abs() < 1e-9);

    // Clamped to [0, 1]
    assert_eq!(RelevanceRanker::combined_score(2.0, 2.0), 1.0);
}

// ---------------------------------------------------------------------------
// ContextSerializer tests
// ---------------------------------------------------------------------------

fn sample_result() -> ContextResult {
    ContextResult {
        items: vec![
            ContextItem {
                entity_id: Uuid::new_v4(),
                entity_type: ContextEntityType::Table,
                relevance_score: 0.95,
                confidence: 0.9,
                label: "fct_orders".into(),
                content: "One row per customer order".into(),
            },
            ContextItem {
                entity_id: Uuid::new_v4(),
                entity_type: ContextEntityType::Column,
                relevance_score: 0.80,
                confidence: 0.85,
                label: "revenue_usd".into(),
                content: "Net revenue in USD after discounts".into(),
            },
        ],
        estimated_tokens: 50,
    }
}

#[test]
fn serialize_markdown_format() {
    let serializer = ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::Markdown,
    };
    let result = sample_result();
    let output = serializer.serialize(&result);

    assert!(output.contains("### fct_orders"));
    assert!(output.contains("confidence: 0.90"));
    assert!(output.contains("relevance: 0.95"));
    assert!(output.contains("One row per customer order"));
    assert!(output.contains("### revenue_usd"));
}

#[test]
fn serialize_jsonl_format() {
    let serializer = ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::JsonLines,
    };
    let result = sample_result();
    let output = serializer.serialize(&result);

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2);

    // Each line should be valid JSON
    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(parsed.get("label").is_some());
        assert!(parsed.get("relevance").is_some());
        assert!(parsed.get("confidence").is_some());
        assert!(parsed.get("content").is_some());
    }
}

#[test]
fn serialize_prose_format() {
    let serializer = ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::Prose,
    };
    let result = sample_result();
    let output = serializer.serialize(&result);

    assert!(output.contains("fct_orders:"));
    assert!(output.contains("revenue_usd:"));
}

#[test]
fn serialize_empty_result() {
    let serializer = ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::Markdown,
    };
    let result = ContextResult::default();
    let output = serializer.serialize(&result);
    assert_eq!(output, "No relevant context found.");

    let prose_serializer = ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::Prose,
    };
    let output = prose_serializer.serialize(&result);
    assert!(output.contains("No relevant context"));
}

#[test]
fn serialize_respects_token_budget() {
    let serializer = ContextSerializer {
        max_tokens: 10, // Very small budget
        format: SerializationFormat::Markdown,
    };

    let result = ContextResult {
        items: vec![
            ContextItem {
                entity_id: Uuid::new_v4(),
                entity_type: ContextEntityType::Table,
                relevance_score: 0.9,
                confidence: 0.9,
                label: "short".into(),
                content: "ok".into(),
            },
            ContextItem {
                entity_id: Uuid::new_v4(),
                entity_type: ContextEntityType::Table,
                relevance_score: 0.8,
                confidence: 0.8,
                label: "long_table_name".into(),
                content: "This is a very long content that should exceed the token budget when combined with the header".into(),
            },
        ],
        estimated_tokens: 100,
    };

    let output = serializer.serialize(&result);
    // Should have truncated — not all items rendered
    assert!(output.len() < 500);
}

#[test]
fn format_table_output() {
    let table = Table {
        id: Uuid::new_v4(),
        schema_id: Uuid::new_v4(),
        name: "fct_orders".into(),
        table_type: TableType::BaseTable,
        description: Some("One row per order".into()),
        dbt_model: None,
        owner: Some("data-eng".into()),
        row_count: Some(1_000_000),
        byte_size: None,
        confidence: 0.85,
        confidence_refreshed_at: None,
        tags: serde_json::json!({}),
        created_at: now(),
        updated_at: now(),
    };

    let columns = vec![Column {
        id: Uuid::new_v4(),
        table_id: table.id,
        name: "order_id".into(),
        data_type: "INT".into(),
        ordinal_position: 0,
        is_nullable: false,
        is_primary_key: true,
        is_foreign_key: false,
        description: Some("Primary key".into()),
        dbt_meta: None,
        tags: serde_json::json!({}),
        confidence: 0.9,
        confidence_refreshed_at: None,
        created_at: now(),
        updated_at: now(),
    }];

    let definitions = vec![SemanticDefinition {
        id: Uuid::new_v4(),
        entity_id: table.id,
        entity_type: SemanticEntityType::Table,
        definition: "Contains all customer orders".into(),
        source: DefinitionSource::DbtYaml,
        confidence: 0.85,
        confidence_refreshed_at: None,
        embedding: None,
        created_at: now(),
        updated_at: now(),
    }];

    let output = ContextSerializer::format_table(&table, &columns, &definitions);

    assert!(output.contains("**Table:** `fct_orders`"));
    assert!(output.contains("**Description:** One row per order"));
    assert!(output.contains("**Owner:** data-eng"));
    assert!(output.contains("**Rows:** 1000000"));
    assert!(output.contains("**Confidence:** 0.85"));
    assert!(output.contains("**Semantic Definitions:**"));
    assert!(output.contains("Contains all customer orders"));
    assert!(output.contains("**Columns:**"));
    assert!(output.contains("| `order_id` | INT | Primary key |"));
}

// ---------------------------------------------------------------------------
// FeedbackRecorder tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feedback_record_interaction() {
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());
    let recorder = FeedbackRecorder::new(store.clone());

    let entity_ids = vec![Uuid::new_v4()];
    let interaction = recorder
        .record_interaction(
            "get_context",
            serde_json::json!({"query": "revenue"}),
            entity_ids.clone(),
            Some("claude".into()),
            Some(150),
        )
        .await
        .unwrap();

    assert_eq!(interaction.tool_name, "get_context");
    assert!(interaction.was_helpful.is_none());
    assert_eq!(interaction.latency_ms, Some(150));
}

#[tokio::test]
async fn feedback_record_and_update() {
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());
    let recorder = FeedbackRecorder::new(store.clone());

    let interaction = recorder
        .record_interaction(
            "describe_table",
            serde_json::json!({"table_ref": "orders"}),
            vec![],
            None,
            None,
        )
        .await
        .unwrap();

    // Mark as helpful
    recorder
        .record_feedback(interaction.id, true)
        .await
        .unwrap();

    // Mark as not helpful (update)
    recorder
        .record_feedback(interaction.id, false)
        .await
        .unwrap();
}
