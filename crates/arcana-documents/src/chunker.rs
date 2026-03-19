use anyhow::Result;
use async_trait::async_trait;
use arcana_core::entities::{Document, DocumentChunk};
use uuid::Uuid;

/// A produced chunk of a document.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub char_start: i64,
    pub char_end: i64,
    pub section_path: Vec<String>,
}

/// A document chunker that splits documents into retrievable pieces.
#[async_trait]
pub trait Chunker: Send + Sync {
    /// Split a document into chunks.
    async fn chunk(&self, document: &Document) -> Result<Vec<Chunk>>;
}

/// A structure-aware chunker that splits on Markdown headings and respects token budgets.
///
/// Strategy:
/// 1. Parse heading hierarchy (H1–H6) to build a section tree.
/// 2. Within each section, split by paragraph boundaries.
/// 3. Merge short paragraphs and split oversized paragraphs to stay within `max_tokens`.
pub struct StructureAwareChunker {
    /// Approximate maximum tokens per chunk (1 token ≈ 4 chars for English).
    pub max_tokens: usize,
    /// Minimum characters before creating a new chunk (avoid tiny orphan chunks).
    pub min_chars: usize,
    /// Character overlap between adjacent chunks for context continuity.
    pub overlap_chars: usize,
}

impl Default for StructureAwareChunker {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            min_chars: 100,
            overlap_chars: 64,
        }
    }
}

#[async_trait]
impl Chunker for StructureAwareChunker {
    async fn chunk(&self, document: &Document) -> Result<Vec<Chunk>> {
        let content = &document.content;
        let max_chars = self.max_tokens * 4; // approximate

        let mut chunks = Vec::new();
        let mut current_section: Vec<String> = Vec::new();
        let mut current_buf = String::new();
        let mut current_start: i64 = 0;
        let mut char_pos: i64 = 0;

        for line in content.lines() {
            let line_len = line.len() as i64 + 1; // +1 for newline

            // Detect heading level
            if let Some(heading_text) = parse_heading(line) {
                let (level, text) = heading_text;

                // Flush current buffer as a chunk
                if current_buf.len() >= self.min_chars {
                    chunks.push(Chunk {
                        content: current_buf.trim().to_string(),
                        char_start: current_start,
                        char_end: char_pos,
                        section_path: current_section.clone(),
                    });
                    current_start = char_pos;
                    current_buf = String::new();
                }

                // Update section path
                if level <= current_section.len() {
                    current_section.truncate(level - 1);
                }
                current_section.push(text.to_string());
            } else {
                current_buf.push_str(line);
                current_buf.push('\n');

                // Split if we've exceeded the max chunk size
                if current_buf.len() > max_chars {
                    chunks.push(Chunk {
                        content: current_buf.trim().to_string(),
                        char_start: current_start,
                        char_end: char_pos + line_len,
                        section_path: current_section.clone(),
                    });
                    // Carry overlap into next chunk
                    let overlap_start = current_buf.len().saturating_sub(self.overlap_chars);
                    current_buf = current_buf[overlap_start..].to_string();
                    current_start = char_pos + line_len - self.overlap_chars as i64;
                }
            }

            char_pos += line_len;
        }

        // Flush any remaining content
        if !current_buf.trim().is_empty() {
            chunks.push(Chunk {
                content: current_buf.trim().to_string(),
                char_start: current_start,
                char_end: char_pos,
                section_path: current_section,
            });
        }

        Ok(chunks)
    }
}

/// Parse a Markdown heading line. Returns `(level, text)` or `None`.
fn parse_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if rest.starts_with(' ') {
        Some((hashes, rest.trim()))
    } else {
        None
    }
}

/// Convert a `Chunk` and a `Document` into a `DocumentChunk` entity.
pub fn to_document_chunk(chunk: Chunk, document_id: Uuid, index: i32) -> DocumentChunk {
    DocumentChunk {
        id: Uuid::new_v4(),
        document_id,
        chunk_index: index,
        content: chunk.content,
        char_start: chunk.char_start,
        char_end: chunk.char_end,
        section_path: chunk.section_path,
        embedding: None,
        created_at: chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcana_core::entities::DocumentSourceType;

    fn make_doc(content: &str) -> Document {
        Document {
            id: Uuid::new_v4(),
            title: "Test".to_string(),
            source_type: DocumentSourceType::Markdown,
            source_uri: "test.md".to_string(),
            raw_content: None,
            content: content.to_string(),
            content_hash: "abc".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn chunks_by_heading() {
        let chunker = StructureAwareChunker::default();
        let content = "# Intro\n\nThis is the intro section with some content.\n\n## Details\n\nThis is the details section.\n";
        let doc = make_doc(content);
        let chunks = chunker.chunk(&doc).await.unwrap();
        assert!(!chunks.is_empty());
        // The details chunk should have the heading in its section_path
        let details_chunk = chunks.iter().find(|c| c.section_path.contains(&"Details".to_string()));
        assert!(details_chunk.is_some());
    }

    #[test]
    fn parse_heading_levels() {
        assert_eq!(parse_heading("# Foo"), Some((1, "Foo")));
        assert_eq!(parse_heading("## Bar"), Some((2, "Bar")));
        assert_eq!(parse_heading("not a heading"), None);
        assert_eq!(parse_heading("#NoSpace"), None);
    }
}
