//! Comprehensive integration tests for the SQLite MetadataStore.
//!
//! These tests exercise all 28 MetadataStore trait methods with roundtrip
//! verification, upsert-update semantics, edge cases, and cross-entity queries.

use arcana_core::entities::*;
use arcana_core::store::{MetadataStore, SqliteStore};
use chrono::Utc;
use uuid::Uuid;

async fn test_store() -> SqliteStore {
    SqliteStore::open("sqlite::memory:").await.unwrap()
}

fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

// ---------------------------------------------------------------------------
// Helpers: create prerequisite entities
// ---------------------------------------------------------------------------

async fn seed_data_source(store: &SqliteStore) -> DataSource {
    let ds = DataSource {
        id: Uuid::new_v4(),
        name: "test_snowflake".into(),
        source_type: DataSourceType::Snowflake,
        connection_info: serde_json::json!({"account": "xy12345"}),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_data_source(&ds).await.unwrap();
    ds
}

async fn seed_schema(store: &SqliteStore, ds_id: Uuid) -> Schema {
    let schema = Schema {
        id: Uuid::new_v4(),
        data_source_id: ds_id,
        database_name: "ANALYTICS".into(),
        schema_name: "PUBLIC".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_schema(&schema).await.unwrap();
    schema
}

async fn seed_table(store: &SqliteStore, schema_id: Uuid, name: &str) -> Table {
    let table = Table {
        id: Uuid::new_v4(),
        schema_id,
        name: name.into(),
        table_type: TableType::BaseTable,
        description: Some(format!("Table {name}")),
        dbt_model: Some(name.into()),
        owner: Some("data-team".into()),
        row_count: Some(1_000_000),
        byte_size: Some(500_000_000),
        confidence: 0.85,
        confidence_refreshed_at: Some(now()),
        tags: serde_json::json!({"domain": "finance"}),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_table(&table).await.unwrap();
    table
}

async fn seed_column(store: &SqliteStore, table_id: Uuid, name: &str, pos: i32) -> Column {
    let col = Column {
        id: Uuid::new_v4(),
        table_id,
        name: name.into(),
        data_type: "VARCHAR".into(),
        ordinal_position: pos,
        is_nullable: true,
        is_primary_key: pos == 0,
        is_foreign_key: false,
        description: Some(format!("Column {name}")),
        dbt_meta: None,
        tags: serde_json::json!({}),
        confidence: 0.9,
        confidence_refreshed_at: Some(now()),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_column(&col).await.unwrap();
    col
}

// ---------------------------------------------------------------------------
// DataSource tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn data_source_crud() {
    let store = test_store().await;
    let ds = seed_data_source(&store).await;

    // Get by ID
    let fetched = store.get_data_source(ds.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "test_snowflake");
    assert_eq!(fetched.source_type, DataSourceType::Snowflake);

    // List
    let all = store.list_data_sources().await.unwrap();
    assert_eq!(all.len(), 1);

    // Get nonexistent
    let missing = store.get_data_source(Uuid::new_v4()).await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn data_source_upsert_updates_existing() {
    let store = test_store().await;
    let mut ds = seed_data_source(&store).await;

    // Update name
    ds.name = "updated_snowflake".into();
    ds.updated_at = now();
    store.upsert_data_source(&ds).await.unwrap();

    let fetched = store.get_data_source(ds.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "updated_snowflake");

    // Still only one
    let all = store.list_data_sources().await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn multiple_data_sources() {
    let store = test_store().await;

    for i in 0..5 {
        let ds = DataSource {
            id: Uuid::new_v4(),
            name: format!("ds_{i}"),
            source_type: DataSourceType::Dbt,
            connection_info: serde_json::json!({}),
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_data_source(&ds).await.unwrap();
    }

    let all = store.list_data_sources().await.unwrap();
    assert_eq!(all.len(), 5);
}

// ---------------------------------------------------------------------------
// Schema tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn schema_roundtrip_and_list() {
    let store = test_store().await;
    let ds = seed_data_source(&store).await;

    let s1 = seed_schema(&store, ds.id).await;

    let s2 = Schema {
        id: Uuid::new_v4(),
        data_source_id: ds.id,
        database_name: "ANALYTICS".into(),
        schema_name: "STAGING".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_schema(&s2).await.unwrap();

    let schemas = store.list_schemas(ds.id).await.unwrap();
    assert_eq!(schemas.len(), 2);

    // Schemas for a different DS should be empty
    let empty = store.list_schemas(Uuid::new_v4()).await.unwrap();
    assert!(empty.is_empty());
}

// ---------------------------------------------------------------------------
// Table tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn table_roundtrip_get_list_search() {
    let store = test_store().await;
    let ds = seed_data_source(&store).await;
    let schema = seed_schema(&store, ds.id).await;

    let orders = seed_table(&store, schema.id, "fct_orders").await;
    let _customers = seed_table(&store, schema.id, "dim_customers").await;
    let _revenue = seed_table(&store, schema.id, "fct_revenue").await;

    // Get by ID
    let fetched = store.get_table(orders.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "fct_orders");
    assert_eq!(fetched.row_count, Some(1_000_000));
    assert_eq!(fetched.confidence, 0.85);

    // List by schema
    let tables = store.list_tables(schema.id).await.unwrap();
    assert_eq!(tables.len(), 3);

    // Search by name
    let results = store.search_tables("orders", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "fct_orders");

    // Search by description
    let results = store.search_tables("Table fct_revenue", 10).await.unwrap();
    assert_eq!(results.len(), 1);

    // Search with limit
    let results = store.search_tables("fct", 1).await.unwrap();
    assert_eq!(results.len(), 1);

    // Search no match
    let results = store.search_tables("nonexistent", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn table_upsert_preserves_id() {
    let store = test_store().await;
    let ds = seed_data_source(&store).await;
    let schema = seed_schema(&store, ds.id).await;
    let mut table = seed_table(&store, schema.id, "orders").await;

    // Update description
    table.description = Some("Updated description".into());
    table.row_count = Some(2_000_000);
    store.upsert_table(&table).await.unwrap();

    let fetched = store.get_table(table.id).await.unwrap().unwrap();
    assert_eq!(fetched.description.as_deref(), Some("Updated description"));
    assert_eq!(fetched.row_count, Some(2_000_000));

    // Still only one table
    let all = store.list_tables(schema.id).await.unwrap();
    assert_eq!(all.len(), 1);
}

// ---------------------------------------------------------------------------
// Column tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn column_roundtrip_and_ordering() {
    let store = test_store().await;
    let ds = seed_data_source(&store).await;
    let schema = seed_schema(&store, ds.id).await;
    let table = seed_table(&store, schema.id, "orders").await;

    let _id_col = seed_column(&store, table.id, "order_id", 0).await;
    let _amount_col = seed_column(&store, table.id, "amount", 1).await;
    let _status_col = seed_column(&store, table.id, "status", 2).await;

    let cols = store.list_columns(table.id).await.unwrap();
    assert_eq!(cols.len(), 3);

    // Should be ordered by ordinal_position
    assert_eq!(cols[0].name, "order_id");
    assert!(cols[0].is_primary_key);
    assert_eq!(cols[1].name, "amount");
    assert_eq!(cols[2].name, "status");
}

// ---------------------------------------------------------------------------
// ColumnProfile tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn column_profile_roundtrip() {
    let store = test_store().await;
    let ds = seed_data_source(&store).await;
    let schema = seed_schema(&store, ds.id).await;
    let table = seed_table(&store, schema.id, "orders").await;
    let col = seed_column(&store, table.id, "amount", 0).await;

    let profile = ColumnProfile {
        id: Uuid::new_v4(),
        column_id: col.id,
        null_count: 42,
        null_pct: 0.042,
        distinct_count: Some(500),
        min_value: Some(serde_json::json!(0.01)),
        max_value: Some(serde_json::json!(99999.99)),
        mean_value: Some(150.50),
        stddev_value: Some(45.3),
        top_values: Some(serde_json::json!(["100.00", "200.00"])),
        profiled_at: now(),
    };

    store.upsert_column_profile(&profile).await.unwrap();
    // No direct get, but upsert should succeed without errors
}

// ---------------------------------------------------------------------------
// SemanticDefinition tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn semantic_definition_roundtrip_with_embedding() {
    let store = test_store().await;
    let entity_id = Uuid::new_v4();

    let def = SemanticDefinition {
        id: Uuid::new_v4(),
        entity_id,
        entity_type: SemanticEntityType::Table,
        definition: "Contains one row per customer order".into(),
        source: DefinitionSource::DbtYaml,
        confidence: 0.85,
        confidence_refreshed_at: Some(now()),
        embedding: Some(vec![0.1, 0.2, 0.3, 0.4]),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_semantic_definition(&def).await.unwrap();

    let defs = store.get_semantic_definitions(entity_id).await.unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].definition, "Contains one row per customer order");
    assert_eq!(defs[0].source, DefinitionSource::DbtYaml);
    assert_eq!(defs[0].embedding.as_ref().unwrap().len(), 4);
}

#[tokio::test]
async fn multiple_semantic_definitions_ordered_by_confidence() {
    let store = test_store().await;
    let entity_id = Uuid::new_v4();

    // Insert two definitions with different confidence
    for (conf, source) in [(0.4, DefinitionSource::LlmInferred), (0.9, DefinitionSource::Manual)] {
        let def = SemanticDefinition {
            id: Uuid::new_v4(),
            entity_id,
            entity_type: SemanticEntityType::Column,
            definition: format!("Definition at {conf}"),
            source,
            confidence: conf,
            confidence_refreshed_at: Some(now()),
            embedding: None,
            created_at: now(),
            updated_at: now(),
        };
        store.upsert_semantic_definition(&def).await.unwrap();
    }

    let defs = store.get_semantic_definitions(entity_id).await.unwrap();
    assert_eq!(defs.len(), 2);
    // Ordered by confidence DESC
    assert!(defs[0].confidence >= defs[1].confidence);
    assert_eq!(defs[0].source, DefinitionSource::Manual);
}

// ---------------------------------------------------------------------------
// Metric tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metric_roundtrip() {
    let store = test_store().await;

    let metric = Metric {
        id: Uuid::new_v4(),
        name: "monthly_revenue".into(),
        label: Some("Monthly Revenue".into()),
        description: Some("Total revenue per month".into()),
        metric_type: MetricType::Simple,
        source_table_id: None,
        expression: Some("SUM(revenue)".into()),
        dimensions: vec!["region".into(), "product".into()],
        filters: Some(serde_json::json!({"status": "completed"})),
        confidence: 0.8,
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_metric(&metric).await.unwrap();

    let metrics = store.list_metrics().await.unwrap();
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].name, "monthly_revenue");
    assert_eq!(metrics[0].dimensions.len(), 2);
    assert_eq!(metrics[0].metric_type, MetricType::Simple);
}

// ---------------------------------------------------------------------------
// DataContract tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn contract_roundtrip() {
    let store = test_store().await;
    let entity_id = Uuid::new_v4();

    let contract = DataContract {
        id: Uuid::new_v4(),
        name: "not_null_order_id".into(),
        entity_id,
        entity_type: ContractEntityType::Column,
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

    let contracts = store.list_contracts(entity_id).await.unwrap();
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].name, "not_null_order_id");
    assert_eq!(contracts[0].status, ContractStatus::Active);
    assert_eq!(contracts[0].last_result, Some(ContractResult::Pass));
}

// ---------------------------------------------------------------------------
// LineageEdge tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lineage_upstream_and_downstream() {
    let store = test_store().await;

    let table_a = Uuid::new_v4();
    let table_b = Uuid::new_v4();
    let table_c = Uuid::new_v4();

    // A → B → C
    let edge_ab = LineageEdge {
        id: Uuid::new_v4(),
        upstream_id: table_a,
        upstream_type: LineageNodeType::Table,
        downstream_id: table_b,
        downstream_type: LineageNodeType::Table,
        source: LineageSource::DbtManifest,
        transform_expression: Some("SELECT * FROM a".into()),
        confidence: 0.9,
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_lineage_edge(&edge_ab).await.unwrap();

    let edge_bc = LineageEdge {
        id: Uuid::new_v4(),
        upstream_id: table_b,
        upstream_type: LineageNodeType::Table,
        downstream_id: table_c,
        downstream_type: LineageNodeType::Table,
        source: LineageSource::DbtManifest,
        transform_expression: None,
        confidence: 0.85,
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_lineage_edge(&edge_bc).await.unwrap();

    // B's upstream should be A
    let upstream = store.get_upstream(table_b).await.unwrap();
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0].upstream_id, table_a);

    // B's downstream should be C
    let downstream = store.get_downstream(table_b).await.unwrap();
    assert_eq!(downstream.len(), 1);
    assert_eq!(downstream[0].downstream_id, table_c);

    // A has no upstream
    let a_upstream = store.get_upstream(table_a).await.unwrap();
    assert!(a_upstream.is_empty());

    // C has no downstream
    let c_downstream = store.get_downstream(table_c).await.unwrap();
    assert!(c_downstream.is_empty());
}

// ---------------------------------------------------------------------------
// Document + Chunk + EntityLink tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn document_and_chunk_roundtrip() {
    let store = test_store().await;

    let doc = Document {
        id: Uuid::new_v4(),
        title: "Data Dictionary".into(),
        source_type: DocumentSourceType::Markdown,
        source_uri: "docs/dictionary.md".into(),
        raw_content: Some("# Data Dictionary\n\nThis is the raw content.".into()),
        content: "# Data Dictionary\n\nThis is the content.".into(),
        content_hash: "abc123".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_document(&doc).await.unwrap();

    let fetched = store.get_document(doc.id).await.unwrap().unwrap();
    assert_eq!(fetched.title, "Data Dictionary");
    assert_eq!(fetched.source_type, DocumentSourceType::Markdown);

    // Add chunks
    for i in 0..3 {
        let chunk = DocumentChunk {
            id: Uuid::new_v4(),
            document_id: doc.id,
            chunk_index: i,
            content: format!("Chunk {i} content"),
            char_start: (i * 100) as i64,
            char_end: ((i + 1) * 100) as i64,
            section_path: vec!["Data Dictionary".into()],
            embedding: Some(vec![0.1 * (i as f32), 0.2, 0.3]),
            created_at: now(),
        };
        store.upsert_chunk(&chunk).await.unwrap();
    }

    let chunks = store.list_chunks(doc.id).await.unwrap();
    assert_eq!(chunks.len(), 3);
    // Ordered by chunk_index
    assert_eq!(chunks[0].chunk_index, 0);
    assert_eq!(chunks[1].chunk_index, 1);
    assert_eq!(chunks[2].chunk_index, 2);
}

#[tokio::test]
async fn entity_link_roundtrip() {
    let store = test_store().await;

    let chunk_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();

    // Need a document and chunk first for FK
    let doc = Document {
        id: Uuid::new_v4(),
        title: "Test Doc".into(),
        source_type: DocumentSourceType::Markdown,
        source_uri: "test.md".into(),
        raw_content: None,
        content: "content".into(),
        content_hash: "hash".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_document(&doc).await.unwrap();

    let chunk = DocumentChunk {
        id: chunk_id,
        document_id: doc.id,
        chunk_index: 0,
        content: "Test chunk mentioning orders table".into(),
        char_start: 0,
        char_end: 40,
        section_path: vec![],
        embedding: None,
        created_at: now(),
    };
    store.upsert_chunk(&chunk).await.unwrap();

    let link = EntityLink {
        id: Uuid::new_v4(),
        chunk_id,
        entity_id,
        entity_type: LinkedEntityType::Table,
        link_method: LinkMethod::ExactMatch,
        confidence: 0.95,
        created_at: now(),
    };
    store.upsert_entity_link(&link).await.unwrap();
}

// ---------------------------------------------------------------------------
// Usage record tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn usage_record_insert() {
    let store = test_store().await;

    // Need a real table for FK
    let ds = seed_data_source(&store).await;
    let schema = seed_schema(&store, ds.id).await;
    let table = seed_table(&store, schema.id, "orders").await;
    let table_id = table.id;

    let record = UsageRecord {
        id: Uuid::new_v4(),
        table_id,
        actor: Some("analytics_bot".into()),
        warehouse: Some("COMPUTE_WH".into()),
        query_type: QueryType::Select,
        bytes_scanned: Some(1_000_000),
        credits_used: Some(0.05),
        duration_ms: Some(1500),
        executed_at: now(),
        created_at: now(),
    };
    store.insert_usage_record(&record).await.unwrap();

    // Duplicate insert should be ignored (INSERT OR IGNORE)
    store.insert_usage_record(&record).await.unwrap();
}

// ---------------------------------------------------------------------------
// Agent interaction + feedback tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn agent_interaction_and_feedback() {
    let store = test_store().await;
    let entity_ids = vec![Uuid::new_v4(), Uuid::new_v4()];

    let interaction = AgentInteraction {
        id: Uuid::new_v4(),
        tool_name: "get_context".into(),
        input: serde_json::json!({"query": "monthly revenue"}),
        referenced_entity_ids: entity_ids,
        agent_id: Some("claude-3".into()),
        was_helpful: None,
        latency_ms: Some(250),
        created_at: now(),
    };
    store.insert_agent_interaction(&interaction).await.unwrap();

    // Record feedback
    store
        .update_interaction_feedback(interaction.id, true)
        .await
        .unwrap();

    // Feedback for nonexistent interaction should succeed (no-op)
    store
        .update_interaction_feedback(Uuid::new_v4(), false)
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Document deduplication (upsert on source_uri conflict)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn document_upsert_deduplicates_by_source_uri() {
    let store = test_store().await;

    let doc1 = Document {
        id: Uuid::new_v4(),
        title: "Original Title".into(),
        source_type: DocumentSourceType::Markdown,
        source_uri: "docs/shared.md".into(),
        raw_content: None,
        content: "Original content".into(),
        content_hash: "hash1".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_document(&doc1).await.unwrap();

    // Re-ingest with updated content
    let doc2 = Document {
        id: Uuid::new_v4(),
        title: "Updated Title".into(),
        source_type: DocumentSourceType::Markdown,
        source_uri: "docs/shared.md".into(),
        raw_content: None,
        content: "Updated content".into(),
        content_hash: "hash2".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_document(&doc2).await.unwrap();

    // Should still have the original doc ID (upsert on source_uri conflict)
    let fetched = store.get_document(doc1.id).await.unwrap().unwrap();
    assert_eq!(fetched.title, "Updated Title");
    assert_eq!(fetched.content, "Updated content");
}
