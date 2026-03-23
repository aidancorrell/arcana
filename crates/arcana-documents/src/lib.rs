//! # arcana-documents
//!
//! Document ingestion pipeline: fetch → chunk → link → embed. Indexes Markdown
//! files (and eventually Confluence, Notion, Slack) into the Arcana store,
//! linking document chunks to known tables and columns.

/// Heading-aware document chunker.
pub mod chunker;
/// Entity linker — resolves table/column references in document text.
pub mod linker;
/// Orchestrates the full ingest pipeline.
pub mod pipeline;
/// The [`DocumentSource`] trait.
pub mod source;
/// Concrete source implementations (Markdown, etc.).
pub mod sources;

pub use chunker::{Chunk, Chunker, StructureAwareChunker};
pub use linker::EntityLinker;
pub use pipeline::{IngestPipeline, IngestResult};
pub use source::DocumentSource;
