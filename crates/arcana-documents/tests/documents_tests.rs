//! Integration tests for the document ingestion pipeline: chunker, linker, and pipeline.

use anyhow::Result;
use arcana_core::{
    embeddings::EmbeddingProvider,
    entities::*,
    store::{MetadataStore, SqliteStore},
};
use arcana_documents::{
    chunker::{Chunk, Chunker, StructureAwareChunker},
    linker::{EntityCandidate, EntityLinker},
    pipeline::{IngestPipeline, IngestResult},
    source::DocumentSource,
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
        // Simple deterministic embedding
        let mut vec = vec![0.0f32; 8];
        for (i, byte) in text.bytes().enumerate() {
            vec[i % 8] += (byte as f32) / 255.0;
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
        8
    }

    fn name(&self) -> &str {
        "mock"
    }
}

// ---------------------------------------------------------------------------
// Mock document source
// ---------------------------------------------------------------------------

struct MockDocumentSource {
    documents: Vec<Document>,
}

#[async_trait]
impl DocumentSource for MockDocumentSource {
    fn name(&self) -> &str {
        "mock"
    }

    async fn fetch_documents(&self) -> Result<Vec<Document>> {
        Ok(self.documents.clone())
    }
}

fn make_doc(title: &str, content: &str) -> Document {
    Document {
        id: Uuid::new_v4(),
        title: title.into(),
        source_type: DocumentSourceType::Markdown,
        source_uri: format!("docs/{title}.md"),
        raw_content: None,
        content: content.into(),
        content_hash: format!("{:x}", md5_hash(content)),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn md5_hash(s: &str) -> u64 {
    let mut hash = 5381u64;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

// ---------------------------------------------------------------------------
// StructureAwareChunker tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chunker_empty_document() {
    let chunker = StructureAwareChunker::default();
    let doc = make_doc("empty", "");
    let chunks = chunker.chunk(&doc).await.unwrap();
    assert!(chunks.is_empty());
}

#[tokio::test]
async fn chunker_single_paragraph() {
    let chunker = StructureAwareChunker {
        max_tokens: 512,
        min_chars: 10,
        overlap_chars: 0,
    };
    let content = "This is a single paragraph with enough content to pass the minimum character threshold for chunking.";
    let doc = make_doc("single", content);
    let chunks = chunker.chunk(&doc).await.unwrap();
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].content.contains("single paragraph"));
}

#[tokio::test]
async fn chunker_splits_on_headings() {
    let chunker = StructureAwareChunker {
        max_tokens: 512,
        min_chars: 20,
        overlap_chars: 0,
    };
    let content = r#"# Introduction

This is the intro with enough text to create a chunk.

# Revenue Tables

These tables contain financial data for reporting.

# Customer Tables

These tables contain customer information.
"#;
    let doc = make_doc("multi-heading", content);
    let chunks = chunker.chunk(&doc).await.unwrap();

    // Should have multiple chunks split by headings
    assert!(chunks.len() >= 2);

    // Check section paths
    let revenue_chunk = chunks
        .iter()
        .find(|c| c.content.contains("financial data"));
    assert!(revenue_chunk.is_some());
    assert!(revenue_chunk
        .unwrap()
        .section_path
        .contains(&"Revenue Tables".to_string()));
}

#[tokio::test]
async fn chunker_nested_headings() {
    let chunker = StructureAwareChunker {
        max_tokens: 512,
        min_chars: 20,
        overlap_chars: 0,
    };
    let content = r#"# Top Level

Some introduction text for the top level.

## Sub Level

This is the sub level with important details.

### Deep Level

This is deeply nested content with its own section.
"#;
    let doc = make_doc("nested", content);
    let chunks = chunker.chunk(&doc).await.unwrap();

    // Deep level chunk should have full section path
    let deep = chunks
        .iter()
        .find(|c| c.content.contains("deeply nested"));
    assert!(deep.is_some());
    let deep = deep.unwrap();
    assert!(deep.section_path.contains(&"Top Level".to_string()));
    assert!(deep.section_path.contains(&"Sub Level".to_string()));
    assert!(deep.section_path.contains(&"Deep Level".to_string()));
}

#[tokio::test]
async fn chunker_large_section_splits() {
    let chunker = StructureAwareChunker {
        max_tokens: 50, // Very small: ~200 chars
        min_chars: 10,
        overlap_chars: 16,
    };
    let content = format!(
        "# Big Section\n\n{}",
        "This is a long paragraph that should be split across multiple chunks. ".repeat(20)
    );
    let doc = make_doc("big", &content);
    let chunks = chunker.chunk(&doc).await.unwrap();
    assert!(chunks.len() > 1, "Long section should be split into multiple chunks");
}

#[tokio::test]
async fn chunker_preserves_char_offsets() {
    let chunker = StructureAwareChunker {
        max_tokens: 512,
        min_chars: 10,
        overlap_chars: 0,
    };
    let content = "# Section A\n\nContent for section A is here.\n\n# Section B\n\nContent for section B is here.\n";
    let doc = make_doc("offsets", content);
    let chunks = chunker.chunk(&doc).await.unwrap();

    for chunk in &chunks {
        assert!(chunk.char_start >= 0);
        assert!(chunk.char_end > chunk.char_start);
    }
}

// ---------------------------------------------------------------------------
// EntityLinker tests
// ---------------------------------------------------------------------------

#[test]
fn linker_exact_match_backtick() {
    let candidates = vec![EntityCandidate {
        id: Uuid::new_v4(),
        entity_type: LinkedEntityType::Table,
        name: "orders".into(),
        aliases: vec![],
    }];
    let linker = EntityLinker::new(candidates.clone(), 0.6);

    let chunk = DocumentChunk {
        id: Uuid::new_v4(),
        document_id: Uuid::new_v4(),
        chunk_index: 0,
        content: "The `orders` table contains all orders.".into(),
        char_start: 0,
        char_end: 40,
        section_path: vec![],
        embedding: None,
        created_at: now(),
    };

    let links = linker.link_chunk(&chunk);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].entity_id, candidates[0].id);
    assert_eq!(links[0].link_method, LinkMethod::ExactMatch);
    assert_eq!(links[0].confidence, 0.95);
}

#[test]
fn linker_exact_match_word_boundary() {
    let candidates = vec![EntityCandidate {
        id: Uuid::new_v4(),
        entity_type: LinkedEntityType::Table,
        name: "orders".into(),
        aliases: vec![],
    }];
    let linker = EntityLinker::new(candidates.clone(), 0.6);

    let chunk = DocumentChunk {
        id: Uuid::new_v4(),
        document_id: Uuid::new_v4(),
        chunk_index: 0,
        content: "Look at the orders table for details.".into(),
        char_start: 0,
        char_end: 40,
        section_path: vec![],
        embedding: None,
        created_at: now(),
    };

    let links = linker.link_chunk(&chunk);
    assert_eq!(links.len(), 1);
}

#[test]
fn linker_no_match_within_word() {
    let candidates = vec![EntityCandidate {
        id: Uuid::new_v4(),
        entity_type: LinkedEntityType::Table,
        name: "orders".into(),
        aliases: vec![],
    }];
    let linker = EntityLinker::new(candidates, 0.6);

    let chunk = DocumentChunk {
        id: Uuid::new_v4(),
        document_id: Uuid::new_v4(),
        chunk_index: 0,
        content: "Check the preorders and reorders tables.".into(),
        char_start: 0,
        char_end: 40,
        section_path: vec![],
        embedding: None,
        created_at: now(),
    };

    let links = linker.link_chunk(&chunk);
    assert!(links.is_empty());
}

#[test]
fn linker_alias_match() {
    let candidates = vec![EntityCandidate {
        id: Uuid::new_v4(),
        entity_type: LinkedEntityType::Table,
        name: "fct_orders".into(),
        aliases: vec!["orders".into(), "order_facts".into()],
    }];
    let linker = EntityLinker::new(candidates.clone(), 0.6);

    let chunk = DocumentChunk {
        id: Uuid::new_v4(),
        document_id: Uuid::new_v4(),
        chunk_index: 0,
        content: "The orders table has one row per order.".into(),
        char_start: 0,
        char_end: 40,
        section_path: vec![],
        embedding: None,
        created_at: now(),
    };

    let links = linker.link_chunk(&chunk);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].entity_id, candidates[0].id);
}

#[test]
fn linker_multiple_entities_in_chunk() {
    let orders_id = Uuid::new_v4();
    let customers_id = Uuid::new_v4();
    let candidates = vec![
        EntityCandidate {
            id: orders_id,
            entity_type: LinkedEntityType::Table,
            name: "orders".into(),
            aliases: vec![],
        },
        EntityCandidate {
            id: customers_id,
            entity_type: LinkedEntityType::Table,
            name: "customers".into(),
            aliases: vec![],
        },
    ];
    let linker = EntityLinker::new(candidates, 0.6);

    let chunk = DocumentChunk {
        id: Uuid::new_v4(),
        document_id: Uuid::new_v4(),
        chunk_index: 0,
        content: "Join orders with customers to get customer names.".into(),
        char_start: 0,
        char_end: 50,
        section_path: vec![],
        embedding: None,
        created_at: now(),
    };

    let links = linker.link_chunk(&chunk);
    assert_eq!(links.len(), 2);

    let entity_ids: Vec<Uuid> = links.iter().map(|l| l.entity_id).collect();
    assert!(entity_ids.contains(&orders_id));
    assert!(entity_ids.contains(&customers_id));
}

#[test]
fn linker_deduplicates_same_entity() {
    let id = Uuid::new_v4();
    let candidates = vec![EntityCandidate {
        id,
        entity_type: LinkedEntityType::Table,
        name: "orders".into(),
        aliases: vec!["orders".into()], // Duplicate alias
    }];
    let linker = EntityLinker::new(candidates, 0.6);

    let chunk = DocumentChunk {
        id: Uuid::new_v4(),
        document_id: Uuid::new_v4(),
        chunk_index: 0,
        content: "The orders table (see orders docs).".into(),
        char_start: 0,
        char_end: 40,
        section_path: vec![],
        embedding: None,
        created_at: now(),
    };

    let links = linker.link_chunk(&chunk);
    assert_eq!(links.len(), 1); // Deduplicated
}

#[test]
fn linker_case_insensitive() {
    let candidates = vec![EntityCandidate {
        id: Uuid::new_v4(),
        entity_type: LinkedEntityType::Table,
        name: "ORDERS".into(),
        aliases: vec![],
    }];
    let linker = EntityLinker::new(candidates.clone(), 0.6);

    let chunk = DocumentChunk {
        id: Uuid::new_v4(),
        document_id: Uuid::new_v4(),
        chunk_index: 0,
        content: "The orders table is important.".into(),
        char_start: 0,
        char_end: 30,
        section_path: vec![],
        embedding: None,
        created_at: now(),
    };

    let links = linker.link_chunk(&chunk);
    assert_eq!(links.len(), 1);
}

// ---------------------------------------------------------------------------
// IngestPipeline integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_ingests_single_document() {
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());
    let provider = Arc::new(MockEmbeddingProvider);
    let chunker = Arc::new(StructureAwareChunker {
        max_tokens: 512,
        min_chars: 10,
        overlap_chars: 0,
    });

    // Seed a table entity for linking
    let ds = DataSource {
        id: Uuid::new_v4(),
        name: "ds".into(),
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
    let table = Table {
        id: Uuid::new_v4(),
        schema_id: schema.id,
        name: "orders".into(),
        table_type: TableType::BaseTable,
        description: None,
        dbt_model: None,
        owner: None,
        row_count: None,
        byte_size: None,
        confidence: 0.8,
        confidence_refreshed_at: None,
        tags: serde_json::json!({}),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_table(&table).await.unwrap();

    let candidates = vec![EntityCandidate {
        id: table.id,
        entity_type: LinkedEntityType::Table,
        name: "orders".into(),
        aliases: vec![],
    }];
    let linker = Arc::new(EntityLinker::new(candidates, 0.6));

    let pipeline = IngestPipeline::new(store.clone(), provider, chunker, linker);

    let source = MockDocumentSource {
        documents: vec![make_doc(
            "Data Dictionary",
            "# Orders\n\nThe orders table contains one row per order.",
        )],
    };

    let result = pipeline.ingest_source(&source).await.unwrap();
    assert_eq!(result.documents_processed, 1);
    assert!(result.chunks_created > 0);
    assert!(result.embeddings_generated > 0);
    // The chunk should link to the orders table
    assert!(result.entity_links_created > 0);
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn pipeline_ingests_multiple_documents() {
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());
    let provider = Arc::new(MockEmbeddingProvider);
    let chunker = Arc::new(StructureAwareChunker {
        max_tokens: 512,
        min_chars: 10,
        overlap_chars: 0,
    });
    let linker = Arc::new(EntityLinker::new(vec![], 0.6));
    let pipeline = IngestPipeline::new(store.clone(), provider, chunker, linker);

    let source = MockDocumentSource {
        documents: vec![
            make_doc("Doc 1", "# First\n\nContent for the first document."),
            make_doc("Doc 2", "# Second\n\nContent for the second document."),
            make_doc("Doc 3", "# Third\n\nContent for the third document."),
        ],
    };

    let result = pipeline.ingest_source(&source).await.unwrap();
    assert_eq!(result.documents_processed, 3);
    assert!(result.chunks_created >= 3);
}

#[tokio::test]
async fn pipeline_empty_source() {
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());
    let provider = Arc::new(MockEmbeddingProvider);
    let chunker = Arc::new(StructureAwareChunker::default());
    let linker = Arc::new(EntityLinker::new(vec![], 0.6));
    let pipeline = IngestPipeline::new(store, provider, chunker, linker);

    let source = MockDocumentSource { documents: vec![] };
    let result = pipeline.ingest_source(&source).await.unwrap();
    assert_eq!(result.documents_processed, 0);
    assert_eq!(result.chunks_created, 0);
}

#[tokio::test]
async fn pipeline_persists_document_and_chunks() {
    let store = Arc::new(SqliteStore::open("sqlite::memory:").await.unwrap());
    let provider = Arc::new(MockEmbeddingProvider);
    let chunker = Arc::new(StructureAwareChunker {
        max_tokens: 512,
        min_chars: 10,
        overlap_chars: 0,
    });
    let linker = Arc::new(EntityLinker::new(vec![], 0.6));
    let pipeline = IngestPipeline::new(store.clone(), provider, chunker, linker);

    let doc = make_doc("test", "# Title\n\nThis document has some content to chunk.");

    let source = MockDocumentSource {
        documents: vec![doc.clone()],
    };
    pipeline.ingest_source(&source).await.unwrap();

    // Verify document was persisted
    let fetched = store.get_document(doc.id).await.unwrap().unwrap();
    assert_eq!(fetched.title, "test");

    // Verify chunks were persisted
    let chunks = store.list_chunks(doc.id).await.unwrap();
    assert!(!chunks.is_empty());

    // Chunks should have embeddings
    for chunk in &chunks {
        assert!(chunk.embedding.is_some());
        assert_eq!(chunk.embedding.as_ref().unwrap().len(), 8); // MockEmbeddingProvider dims
    }
}
