use anyhow::{Context, Result};
use async_trait::async_trait;
use arcana_core::entities::{Document, DocumentSourceType};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::source::DocumentSource;

/// Discovers and reads Markdown (`.md`) files from one or more directory globs.
pub struct MarkdownSource {
    /// Glob patterns to search for Markdown files (e.g., `["docs/**/*.md"]`).
    globs: Vec<String>,
}

impl MarkdownSource {
    pub fn new(globs: Vec<String>) -> Self {
        Self { globs }
    }

    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            globs: vec![format!("{}/**/*.md", path.display())],
        }
    }

    async fn read_file(&self, path: &Path) -> Result<Document> {
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read Markdown file at {:?}", path))?;

        // Extract title from first H1 heading, fall back to filename.
        let title = extract_title(&content)
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string()
            });

        let content_hash = sha256_hex(&content);
        let now = chrono::Utc::now();

        Ok(Document {
            id: Uuid::new_v4(),
            title,
            source_type: DocumentSourceType::Markdown,
            source_uri: path.display().to_string(),
            raw_content: Some(content.clone()),
            content,
            content_hash,
            created_at: now,
            updated_at: now,
        })
    }
}

#[async_trait]
impl DocumentSource for MarkdownSource {
    fn name(&self) -> &str {
        "markdown"
    }

    async fn fetch_documents(&self) -> Result<Vec<Document>> {
        let mut documents = Vec::new();

        for pattern in &self.globs {
            let paths = glob::glob(pattern)
                .with_context(|| format!("invalid glob pattern: {pattern}"))?;

            for entry in paths {
                let path = entry.context("failed to expand glob entry")?;
                if path.is_file() {
                    match self.read_file(&path).await {
                        Ok(doc) => documents.push(doc),
                        Err(e) => {
                            tracing::warn!("skipping {:?}: {e}", path);
                        }
                    }
                }
            }
        }

        Ok(documents)
    }
}

/// Extract the first H1 heading from Markdown content.
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("# ") {
            return Some(heading.trim().to_string());
        }
    }
    None
}

/// Compute SHA-256 hex digest of a string (for change detection).
fn sha256_hex(content: &str) -> String {
    use sha2::{Sha256, Digest};
    let hash = Sha256::digest(content.as_bytes());
    hex_encode(&hash)
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(s, "{byte:02x}").unwrap();
    }
    s
}
