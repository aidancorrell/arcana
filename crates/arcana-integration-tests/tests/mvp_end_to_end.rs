//! End-to-end MVP integration test.
//!
//! Simulates the full Arcana workflow: init store → sync metadata (via dbt adapter
//! with fixture data) → ingest documents → query context → describe table →
//! update context → verify feedback loop.
//!
//! This test uses in-memory SQLite and mock embedding providers to avoid
//! external dependencies (no Snowflake, no OpenAI).

use anyhow::Result;
use arcana_core::{
    confidence::ConfidenceDecay,
    embeddings::{EmbeddingProvider, VectorIndex},
    entities::*,
    store::{MetadataStore, SqliteStore},
};
use arcana_documents::{
    chunker::StructureAwareChunker,
    linker::{EntityCandidate, EntityLinker},
    pipeline::IngestPipeline,
    source::DocumentSource,
};
use arcana_mcp::tools::{
    DescribeTableInput, GetContextInput, UpdateContextInput,
    handle_describe_table, handle_get_context, handle_update_context,
};
use arcana_recommender::{
    feedback::FeedbackRecorder,
    ranker::{ContextRequest, RelevanceRanker},
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

impl MockEmbeddingProvider {
    fn new() -> Self {
        Self { dims: 32 }
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vec = vec![0.0f32; self.dims];
        for (i, byte) in text.bytes().enumerate() {
            vec[i % self.dims] += (byte as f32) / 255.0;
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
        self.dims
    }

    fn name(&self) -> &str {
        "mock"
    }
}

// ---------------------------------------------------------------------------
// Mock document source (simulates Markdown files)
// ---------------------------------------------------------------------------

struct MockDocumentSource {
    documents: Vec<Document>,
}

#[async_trait]
impl DocumentSource for MockDocumentSource {
    fn name(&self) -> &str {
        "markdown"
    }

    async fn fetch_documents(&self) -> Result<Vec<Document>> {
        Ok(self.documents.clone())
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

/// Simulate what a dbt adapter sync would produce.
async fn simulate_dbt_sync(store: &SqliteStore) -> (Vec<Table>, Vec<Column>) {
    let ds = DataSource {
        id: Uuid::new_v4(),
        name: "dbt_project".into(),
        source_type: DataSourceType::Dbt,
        connection_info: serde_json::json!({"project_path": "/path/to/dbt"}),
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

    // Simulate 4 tables that a real dbt project might produce
    let table_specs = vec![
        ("fct_orders", "One row per customer order", 5_000_000i64),
        ("fct_revenue", "Monthly revenue aggregation", 120_000),
        ("dim_customers", "Customer master data", 500_000),
        ("dim_products", "Product catalog", 10_000),
    ];

    let mut tables = Vec::new();
    let mut all_columns = Vec::new();

    for (name, desc, rows) in table_specs {
        let table = Table {
            id: Uuid::new_v4(),
            schema_id: schema.id,
            name: name.into(),
            table_type: TableType::BaseTable,
            description: Some(desc.into()),
            dbt_model: Some(name.into()),
            owner: Some("data-eng".into()),
            row_count: Some(rows),
            byte_size: Some(rows * 500),
            confidence: 0.80,
            confidence_refreshed_at: Some(now()),
            tags: serde_json::json!({"domain": "analytics"}),
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_table(&table).await.unwrap();

        // Add a semantic definition (from dbt schema YAML)
        let def = SemanticDefinition {
            id: Uuid::new_v4(),
            entity_id: table.id,
            entity_type: SemanticEntityType::Table,
            definition: desc.into(),
            source: DefinitionSource::DbtYaml,
            confidence: 0.80,
            confidence_refreshed_at: Some(now()),
            embedding: None,
            definition_hash: None,
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_semantic_definition(&def).await.unwrap();

        // Add columns
        let col_specs: Vec<(&str, &str, bool)> = match name {
            "fct_orders" => vec![
                ("order_id", "INT", false),
                ("customer_id", "INT", true),
                ("amount", "DECIMAL(12,2)", true),
                ("status", "VARCHAR", true),
                ("created_at", "TIMESTAMP", true),
            ],
            "fct_revenue" => vec![
                ("month", "DATE", false),
                ("region", "VARCHAR", true),
                ("revenue_usd", "DECIMAL(18,2)", true),
            ],
            "dim_customers" => vec![
                ("customer_id", "INT", false),
                ("name", "VARCHAR", true),
                ("email", "VARCHAR", true),
                ("region", "VARCHAR", true),
            ],
            "dim_products" => vec![
                ("product_id", "INT", false),
                ("name", "VARCHAR", true),
                ("category", "VARCHAR", true),
                ("price", "DECIMAL(10,2)", true),
            ],
            _ => vec![],
        };

        for (i, (col_name, dtype, nullable)) in col_specs.iter().enumerate() {
            let col = Column {
                id: Uuid::new_v4(),
                table_id: table.id,
                name: col_name.to_string(),
                data_type: dtype.to_string(),
                ordinal_position: i as i32,
                is_nullable: *nullable,
                is_primary_key: i == 0,
                is_foreign_key: false,
                description: Some(format!("The {col_name} field")),
                dbt_meta: None,
                tags: serde_json::json!({}),
                confidence: 0.85,
                confidence_refreshed_at: Some(now()),
                created_at: now(),
                updated_at: now(),
            };
            store.upsert_column(&col).await.unwrap();
            all_columns.push(col);
        }

        tables.push(table);
    }

    // Add lineage: dim_customers → fct_orders
    let edge = LineageEdge {
        id: Uuid::new_v4(),
        upstream_id: tables[2].id,  // dim_customers
        upstream_type: LineageNodeType::Table,
        downstream_id: tables[0].id, // fct_orders
        downstream_type: LineageNodeType::Table,
        source: LineageSource::DbtManifest,
        transform_expression: Some("JOIN on customer_id".into()),
        confidence: 0.9,
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_lineage_edge(&edge).await.unwrap();

    // Add lineage: fct_orders → fct_revenue
    let edge2 = LineageEdge {
        id: Uuid::new_v4(),
        upstream_id: tables[0].id,  // fct_orders
        upstream_type: LineageNodeType::Table,
        downstream_id: tables[1].id, // fct_revenue
        downstream_type: LineageNodeType::Table,
        source: LineageSource::DbtManifest,
        transform_expression: Some("GROUP BY month, region".into()),
        confidence: 0.85,
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_lineage_edge(&edge2).await.unwrap();

    // Add a metric
    let metric = Metric {
        id: Uuid::new_v4(),
        name: "monthly_revenue".into(),
        label: Some("Monthly Revenue".into()),
        description: Some("Total revenue aggregated by month and region".into()),
        metric_type: MetricType::Simple,
        source_table_id: Some(tables[1].id),
        expression: Some("SUM(revenue_usd)".into()),
        dimensions: vec!["month".into(), "region".into()],
        filters: None,
        confidence: 0.80,
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_metric(&metric).await.unwrap();

    // Add a data contract
    let contract = DataContract {
        id: Uuid::new_v4(),
        name: "orders_not_null_id".into(),
        entity_id: tables[0].id,
        entity_type: ContractEntityType::Table,
        contract_type: ContractType::NotNull,
        description: Some("order_id must never be null".into()),
        expression: serde_json::json!({"column": "order_id"}),
        status: ContractStatus::Active,
        last_evaluated_at: Some(now()),
        last_result: Some(ContractResult::Pass),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_contract(&contract).await.unwrap();

    (tables, all_columns)
}

// ---------------------------------------------------------------------------
// THE END-TO-END TEST
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mvp_end_to_end() {
    // -----------------------------------------------------------------------
    // Phase 1: Initialize store
    // -----------------------------------------------------------------------
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());

    // Verify empty state
    let sources = store.list_data_sources().await.unwrap();
    assert!(sources.is_empty(), "Fresh store should have no data sources");

    // -----------------------------------------------------------------------
    // Phase 2: Simulate dbt sync (metadata adapter output)
    // -----------------------------------------------------------------------
    let (tables, columns) = simulate_dbt_sync(&store).await;

    // Verify sync produced expected entities
    let sources = store.list_data_sources().await.unwrap();
    assert_eq!(sources.len(), 1, "Should have one data source");

    let all_metrics = store.list_metrics().await.unwrap();
    assert_eq!(all_metrics.len(), 1, "Should have one metric");

    // Search for tables
    let order_tables = store.search_tables("orders", 10).await.unwrap();
    assert_eq!(order_tables.len(), 1);
    assert_eq!(order_tables[0].name, "fct_orders");

    let revenue_tables = store.search_tables("revenue", 10).await.unwrap();
    assert_eq!(revenue_tables.len(), 1);

    // Verify lineage
    let fct_orders = &tables[0];
    let upstream = store.get_upstream(fct_orders.id).await.unwrap();
    assert_eq!(upstream.len(), 1, "fct_orders should have 1 upstream (dim_customers)");

    let downstream = store.get_downstream(fct_orders.id).await.unwrap();
    assert_eq!(downstream.len(), 1, "fct_orders should have 1 downstream (fct_revenue)");

    // Verify contracts
    let contracts = store.list_contracts(fct_orders.id).await.unwrap();
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].last_result, Some(ContractResult::Pass));

    // -----------------------------------------------------------------------
    // Phase 3: Build embedding index (simulating `arcana reembed`)
    // -----------------------------------------------------------------------
    let provider = Arc::new(MockEmbeddingProvider::new());
    let entity_index = Arc::new(VectorIndex::new(provider.dimensions()));
    let chunk_index = Arc::new(VectorIndex::new(provider.dimensions()));

    // Index all tables
    for table in &tables {
        let text = format!("{} {}", table.name, table.description.as_deref().unwrap_or(""));
        let embedding = provider.embed(&text).await.unwrap();
        entity_index.upsert(table.id, embedding).unwrap();
    }
    assert_eq!(entity_index.len(), 4);

    // -----------------------------------------------------------------------
    // Phase 4: Ingest documents
    // -----------------------------------------------------------------------
    let data_dictionary = Document {
        id: Uuid::new_v4(),
        title: "Data Dictionary".into(),
        source_type: DocumentSourceType::Markdown,
        source_uri: "docs/data-dictionary.md".into(),
        raw_content: None,
        content: r#"# Data Dictionary

## Orders

The `fct_orders` table is the primary fact table for customer orders.
Each row represents a single order placed by a customer.

Key columns:
- `order_id`: Unique identifier for the order
- `amount`: Total order amount in USD
- `status`: Current order status (pending, shipped, delivered)

## Revenue

The `fct_revenue` table aggregates revenue by month and region.
It is derived from `fct_orders` and is the source for the monthly_revenue metric.

## Customers

The `dim_customers` table contains customer master data including
contact information and geographic region.
"#
        .into(),
        content_hash: "dict_hash".into(),
        created_at: now(),
        updated_at: now(),
    };

    let candidates: Vec<EntityCandidate> = tables
        .iter()
        .map(|t| EntityCandidate {
            id: t.id,
            entity_type: LinkedEntityType::Table,
            name: t.name.clone(),
            aliases: vec![],
        })
        .collect();

    let chunker = Arc::new(StructureAwareChunker {
        max_tokens: 512,
        min_chars: 30,
        overlap_chars: 32,
    });
    let linker = Arc::new(EntityLinker::new(candidates, 0.6));
    let pipeline = IngestPipeline::new(
        store.clone(),
        provider.clone(),
        chunker,
        linker,
    );

    let source = MockDocumentSource {
        documents: vec![data_dictionary.clone()],
    };
    let ingest_result = pipeline.ingest_source(&source).await.unwrap();

    assert_eq!(ingest_result.documents_processed, 1);
    assert!(ingest_result.chunks_created > 0, "Should create chunks");
    assert!(ingest_result.embeddings_generated > 0, "Should generate embeddings");
    assert!(
        ingest_result.entity_links_created > 0,
        "Should link to tables (fct_orders, fct_revenue, dim_customers)"
    );
    assert!(ingest_result.errors.is_empty(), "No errors expected");

    // Verify document and chunks persisted
    let doc = store.get_document(data_dictionary.id).await.unwrap().unwrap();
    assert_eq!(doc.title, "Data Dictionary");
    let chunks = store.list_chunks(data_dictionary.id).await.unwrap();
    assert!(!chunks.is_empty());
    for chunk in &chunks {
        assert!(chunk.embedding.is_some(), "Each chunk should have an embedding");
    }

    // Index chunks for search
    for chunk in &chunks {
        if let Some(ref emb) = chunk.embedding {
            chunk_index.upsert(chunk.id, emb.clone()).unwrap();
        }
    }

    // -----------------------------------------------------------------------
    // Phase 5: Context recommendation (MCP get_context)
    // -----------------------------------------------------------------------
    let ranker = Arc::new(RelevanceRanker::new(
        store.clone(),
        provider.clone(),
        entity_index.clone(),
        chunk_index.clone(),
    ));
    let serializer = Arc::new(ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::Markdown,
    });

    // Query: "monthly revenue by region"
    let input = GetContextInput {
        query: "monthly revenue by region".into(),
        top_k: 5,
        min_confidence: 0.0,
        expand_lineage: false,
    };
    let context_output = handle_get_context(input, ranker.clone(), serializer.clone()).await.unwrap();

    assert!(context_output.item_count > 0, "Should find relevant context");
    assert!(context_output.estimated_tokens > 0);

    // The ranker should return results — let's also test the ranker directly
    let request = ContextRequest {
        query: "orders placed by customers".into(),
        top_k: 5,
        filter_table_id: None,
        min_confidence: 0.0,
        expand_lineage: false,
    };
    let rank_result = ranker.rank(&request).await.unwrap();
    assert!(!rank_result.items.is_empty(), "Should find tables for 'orders placed by customers'");

    // -----------------------------------------------------------------------
    // Phase 6: Table description (MCP describe_table)
    // -----------------------------------------------------------------------
    let describe_input = DescribeTableInput {
        table_ref: "fct_orders".into(),
        include_columns: true,
        include_definitions: true,
        include_contracts: false,
        include_lineage: false,
    };
    let describe_output = handle_describe_table(describe_input, store.clone()).await.unwrap();

    assert!(describe_output.table_id.is_some());
    assert_eq!(describe_output.confidence, 0.80);
    assert!(describe_output.description.contains("fct_orders"));
    assert!(describe_output.description.contains("order_id"));
    assert!(describe_output.description.contains("One row per customer order"));

    // -----------------------------------------------------------------------
    // Phase 7: Update context (MCP update_context) — agent pushes back knowledge
    // -----------------------------------------------------------------------
    let update_input = UpdateContextInput {
        entity_id: tables[0].id, // fct_orders
        entity_type: "table".into(),
        definition: "Primary fact table for e-commerce orders. Grain: one row per order_id. Joins to dim_customers on customer_id.".into(),
        confidence: 0.75,
    };
    let update_output = handle_update_context(update_input, store.clone()).await.unwrap();
    assert!(update_output.success);

    // Verify the new definition is persisted alongside the original
    let defs = store.get_semantic_definitions(tables[0].id).await.unwrap();
    assert!(defs.len() >= 2, "Should have original + agent-contributed definition");
    let llm_def = defs.iter().find(|d| d.source == DefinitionSource::LlmInferred);
    assert!(llm_def.is_some());
    assert!(llm_def.unwrap().definition.contains("Grain: one row per order_id"));

    // -----------------------------------------------------------------------
    // Phase 8: Feedback loop (agent reports helpfulness)
    // -----------------------------------------------------------------------
    let recorder = FeedbackRecorder::new(store.clone());
    let interaction = recorder
        .record_interaction(
            "get_context",
            serde_json::json!({"query": "monthly revenue by region"}),
            tables.iter().map(|t| t.id).collect(),
            Some("claude-desktop".into()),
            Some(200),
        )
        .await
        .unwrap();
    assert_eq!(interaction.tool_name, "get_context");

    recorder
        .record_feedback(interaction.id, true)
        .await
        .unwrap();

    // -----------------------------------------------------------------------
    // Phase 9: Confidence decay verification
    // -----------------------------------------------------------------------
    let decay = ConfidenceDecay::default();

    // Freshly synced table should not be stale
    let fresh_score = decay.decayed_score(0.80, Some(now()));
    assert!(!decay.is_stale(fresh_score), "Freshly synced should not be stale");

    // Table synced 60 days ago should be stale
    let old_score = decay.decayed_score(0.80, Some(now() - chrono::Duration::days(60)));
    assert!(decay.is_stale(old_score), "60-day-old metadata should be stale");
    assert!(old_score.value() < 0.4);

    // -----------------------------------------------------------------------
    // Phase 10: Serialization formats
    // -----------------------------------------------------------------------
    let request = ContextRequest {
        query: "orders".into(),
        top_k: 3,
        filter_table_id: None,
        min_confidence: 0.0,
        expand_lineage: false,
    };

    // Markdown
    let md_serializer = ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::Markdown,
    };
    let result = ranker.rank(&request).await.unwrap();
    let md_output = md_serializer.serialize(&result);
    if !result.items.is_empty() {
        assert!(md_output.contains("###"), "Markdown should have headings");
    }

    // JSON Lines
    let jsonl_serializer = ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::JsonLines,
    };
    let jsonl_output = jsonl_serializer.serialize(&result);
    if !result.items.is_empty() {
        for line in jsonl_output.lines() {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.get("entity_id").is_some());
        }
    }

    // Prose
    let prose_serializer = ContextSerializer {
        max_tokens: 8000,
        format: SerializationFormat::Prose,
    };
    let prose_output = prose_serializer.serialize(&result);
    assert!(!prose_output.is_empty());

    // -----------------------------------------------------------------------
    // Phase 11: Verify complete entity counts (status command equivalent)
    // -----------------------------------------------------------------------
    let all_sources = store.list_data_sources().await.unwrap();
    assert_eq!(all_sources.len(), 1);

    let metrics = store.list_metrics().await.unwrap();
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].name, "monthly_revenue");

    // We should be able to search for any table
    for expected_name in &["fct_orders", "fct_revenue", "dim_customers", "dim_products"] {
        let results = store.search_tables(expected_name, 1).await.unwrap();
        assert!(!results.is_empty(), "Should find table {expected_name}");
    }

    println!("MVP end-to-end test passed!");
    println!("  - {} data sources", all_sources.len());
    println!("  - {} tables synced", tables.len());
    println!("  - {} columns synced", columns.len());
    println!("  - {} metrics defined", metrics.len());
    println!("  - {} document chunks ingested", chunks.len());
    println!("  - {} entity links created", ingest_result.entity_links_created);
    println!("  - Context recommendation working");
    println!("  - Table description working");
    println!("  - Context update (agent feedback) working");
    println!("  - Confidence decay system verified");
}
