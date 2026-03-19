use anyhow::Result;
use arcana_core::{
    embeddings::EmbeddingProvider,
    entities::{Document, DocumentChunk, EntityLink},
    store::MetadataStore,
};
use std::sync::Arc;

use crate::{
    chunker::{to_document_chunk, Chunker},
    linker::EntityLinker,
    source::DocumentSource,
};

/// Summary of a document ingestion run.
#[derive(Debug, Default)]
pub struct IngestResult {
    pub documents_processed: usize,
    pub documents_skipped: usize,
    pub chunks_created: usize,
    pub entity_links_created: usize,
    pub embeddings_generated: usize,
    pub errors: Vec<String>,
}

/// Orchestrates the full document ingestion pipeline:
///
/// ```text
/// DocumentSource → [Document] → Chunker → [Chunk] → EntityLinker → [EntityLink]
///                                                  → EmbeddingProvider → [Vec<f32>]
///                                                  → MetadataStore (persisted)
/// ```
pub struct IngestPipeline {
    store: Arc<dyn MetadataStore>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    chunker: Arc<dyn Chunker>,
    linker: Arc<EntityLinker>,
}

impl IngestPipeline {
    pub fn new(
        store: Arc<dyn MetadataStore>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        chunker: Arc<dyn Chunker>,
        linker: Arc<EntityLinker>,
    ) -> Self {
        Self {
            store,
            embedding_provider,
            chunker,
            linker,
        }
    }

    /// Ingest all documents from a source.
    pub async fn ingest_source(&self, source: &dyn DocumentSource) -> Result<IngestResult> {
        let mut result = IngestResult::default();

        tracing::info!("fetching documents from source '{}'", source.name());
        let documents = source.fetch_documents().await?;
        tracing::info!("found {} documents", documents.len());

        for doc in documents {
            match self.ingest_document(doc).await {
                Ok((chunks, links)) => {
                    result.documents_processed += 1;
                    result.chunks_created += chunks;
                    result.entity_links_created += links;
                    result.embeddings_generated += chunks;
                }
                Err(e) => {
                    tracing::warn!("failed to ingest document: {e}");
                    result.errors.push(e.to_string());
                }
            }
        }

        Ok(result)
    }

    /// Ingest a single document: chunk → link → embed → persist.
    ///
    /// Returns `(chunk_count, link_count)`.
    async fn ingest_document(&self, document: Document) -> Result<(usize, usize)> {
        tracing::debug!("ingesting document '{}'", document.title);

        // 1. Persist the document
        self.store.upsert_document(&document).await?;

        // 2. Chunk the document
        let raw_chunks = self.chunker.chunk(&document).await?;
        let mut doc_chunks: Vec<DocumentChunk> = raw_chunks
            .into_iter()
            .enumerate()
            .map(|(i, c)| to_document_chunk(c, document.id, i as i32))
            .collect();

        // 3. Embed all chunks (batch)
        let texts: Vec<&str> = doc_chunks.iter().map(|c| c.content.as_str()).collect();
        let embeddings = self.embedding_provider.embed_batch(&texts).await?;

        for (chunk, embedding) in doc_chunks.iter_mut().zip(embeddings) {
            chunk.embedding = Some(embedding);
        }

        // 4. Persist chunks
        for chunk in &doc_chunks {
            self.store.upsert_chunk(chunk).await?;
        }

        // 5. Link chunks to entities
        let mut total_links = 0;
        for chunk in &doc_chunks {
            let links = self.linker.link_chunk(chunk);
            for link in &links {
                self.store.upsert_entity_link(link).await?;
            }
            total_links += links.len();
        }

        tracing::info!(
            "ingested '{}': {} chunks, {} entity links",
            document.title,
            doc_chunks.len(),
            total_links
        );

        Ok((doc_chunks.len(), total_links))
    }
}
