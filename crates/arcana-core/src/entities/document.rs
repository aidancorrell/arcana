use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A source document ingested into Arcana (wiki page, Markdown file, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    pub title: String,
    pub source_type: DocumentSourceType,
    /// Original URL or file path.
    pub source_uri: String,
    /// Raw document content (before chunking).
    pub raw_content: Option<String>,
    /// Parsed/cleaned content.
    pub content: String,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentSourceType {
    Markdown,
    Confluence,
    Notion,
    GoogleDoc,
    PlainText,
    Html,
}

/// A chunk of a document — the unit of embedding and retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub chunk_index: i32,
    pub content: String,
    /// Character offset in the original document.
    pub char_start: i64,
    pub char_end: i64,
    /// Section heading(s) this chunk is nested under.
    pub section_path: Vec<String>,
    /// Embedding vector (stored as JSON for SQLite compat).
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
}

/// A link between a document chunk and a metadata entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityLink {
    pub id: Uuid,
    pub chunk_id: Uuid,
    /// The entity (table, column, metric, etc.) this chunk references.
    pub entity_id: Uuid,
    pub entity_type: LinkedEntityType,
    /// How the link was established.
    pub link_method: LinkMethod,
    /// Confidence that this link is correct (0.0–1.0).
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkedEntityType {
    Table,
    Column,
    Metric,
    DataSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkMethod {
    /// Exact string match (e.g., "`orders`" in markdown → orders table).
    ExactMatch,
    /// Fuzzy string match.
    FuzzyMatch,
    /// LLM identified the reference.
    LlmExtracted,
    /// Manual annotation.
    Manual,
}
