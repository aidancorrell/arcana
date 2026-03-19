use anyhow::Result;
use async_trait::async_trait;
use arcana_core::entities::Document;

/// A source of documents to be ingested into Arcana.
///
/// Implementors discover and yield raw documents (Markdown files, Confluence pages, etc.)
/// that are then processed by the ingestion pipeline.
#[async_trait]
pub trait DocumentSource: Send + Sync {
    /// Human-readable name of this source (e.g., "markdown", "confluence").
    fn name(&self) -> &str;

    /// Discover and fetch all documents from this source.
    async fn fetch_documents(&self) -> Result<Vec<Document>>;

    /// Fetch documents that have changed since the given timestamp.
    /// Default implementation fetches all documents (full refresh).
    async fn fetch_changed_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Document>> {
        let _ = since;
        self.fetch_documents().await
    }
}
