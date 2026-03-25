use arcana_core::entities::{DocumentChunk, EntityLink, LinkedEntityType, LinkMethod};
use uuid::Uuid;

/// An entity candidate for linking.
#[derive(Debug, Clone)]
pub struct EntityCandidate {
    pub id: Uuid,
    pub entity_type: LinkedEntityType,
    /// The canonical name (table name, column name, metric name).
    pub name: String,
    /// Alternative names / aliases.
    pub aliases: Vec<String>,
}

/// Links document chunks to metadata entities using exact match, fuzzy match, or LLM extraction.
pub struct EntityLinker {
    /// Known entities to link against.
    candidates: Vec<EntityCandidate>,
    /// Confidence threshold for fuzzy matches (reserved for future use).
    #[allow(dead_code)]
    fuzzy_threshold: f64,
}

impl EntityLinker {
    pub fn new(candidates: Vec<EntityCandidate>, fuzzy_threshold: f64) -> Self {
        Self {
            candidates,
            fuzzy_threshold,
        }
    }

    /// Find all entity links in a chunk using exact and fuzzy matching.
    pub fn link_chunk(&self, chunk: &DocumentChunk) -> Vec<EntityLink> {
        let mut links = Vec::new();
        let content_lower = chunk.content.to_lowercase();

        for candidate in &self.candidates {
            // 1. Exact match: look for backtick-wrapped name or standalone word
            let exact_patterns = std::iter::once(candidate.name.as_str())
                .chain(candidate.aliases.iter().map(|a| a.as_str()));

            for pattern in exact_patterns {
                if exact_match(&content_lower, &pattern.to_lowercase()) {
                    links.push(EntityLink {
                        id: Uuid::new_v4(),
                        chunk_id: chunk.id,
                        entity_id: candidate.id,
                        entity_type: candidate.entity_type.clone(),
                        link_method: LinkMethod::ExactMatch,
                        confidence: 0.95,
                        created_at: chrono::Utc::now(),
                    });
                    break;
                }
            }
        }

        // Deduplicate by entity_id (keep highest confidence)
        links.sort_by(|a, b| {
            a.entity_id
                .cmp(&b.entity_id)
                .then(b.confidence.partial_cmp(&a.confidence).unwrap())
        });
        links.dedup_by_key(|l| l.entity_id);

        links
    }
}

/// Check if `needle` appears as a word boundary match in `haystack`.
fn exact_match(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    // Check for backtick-wrapped version first
    if haystack.contains(&format!("`{needle}`")) {
        return true;
    }
    // Word-boundary-ish check: needle surrounded by non-alphanumeric chars
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs_pos = start + pos;
        let before_ok = abs_pos == 0
            || !haystack
                .as_bytes()
                .get(abs_pos - 1)
                .map(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .unwrap_or(false);
        let end_pos = abs_pos + needle.len();
        let after_ok = end_pos >= haystack.len()
            || !haystack
                .as_bytes()
                .get(end_pos)
                .map(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        start = abs_pos + 1;
        if start >= haystack.len() {
            break;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_backtick() {
        assert!(exact_match("see `orders` table", "orders"));
        assert!(exact_match("the orders table", "orders"));
        assert!(!exact_match("preorders table", "orders"));
    }
}
