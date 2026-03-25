use arcana_core::entities::{Column, LineageEdge, SemanticDefinition, Table};

use crate::ranker::ContextResult;

/// Serializes a `ContextResult` into a token-efficient string representation
/// suitable for inclusion in an LLM context window.
pub struct ContextSerializer {
    /// Maximum number of tokens to include in the output.
    pub max_tokens: usize,
    /// Output format.
    pub format: SerializationFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    /// Compact Markdown with minimal whitespace.
    Markdown,
    /// JSON Lines for programmatic consumption.
    JsonLines,
    /// Plain prose optimized for language model reading.
    Prose,
}

impl Default for ContextSerializer {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            format: SerializationFormat::Markdown,
        }
    }
}

impl ContextSerializer {
    /// Serialize a `ContextResult` to a string, respecting the token budget.
    pub fn serialize(&self, result: &ContextResult) -> String {
        match self.format {
            SerializationFormat::Markdown => self.to_markdown(result),
            SerializationFormat::JsonLines => self.to_jsonl(result),
            SerializationFormat::Prose => self.to_prose(result),
        }
    }

    fn to_markdown(&self, result: &ContextResult) -> String {
        let mut out = String::new();
        let mut token_budget = self.max_tokens;

        for item in &result.items {
            let section = format!(
                "### {} (confidence: {:.2}, relevance: {:.2})\n{}\n\n",
                item.label,
                item.confidence,
                item.relevance_score,
                item.content
            );
            let section_tokens = section.len() / 4;
            if section_tokens > token_budget {
                break;
            }
            out.push_str(&section);
            token_budget -= section_tokens;
        }

        // Append lineage DAG if present
        if !result.lineage_edges.is_empty() {
            let lineage_section = format_lineage_dag(&result.lineage_edges);
            let lineage_tokens = lineage_section.len() / 4;
            if lineage_tokens <= token_budget {
                out.push_str(&lineage_section);
            }
        }

        if out.is_empty() {
            "No relevant context found.".to_string()
        } else {
            out
        }
    }

    fn to_jsonl(&self, result: &ContextResult) -> String {
        result
            .items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "entity_id": item.entity_id,
                    "label": item.label,
                    "relevance": item.relevance_score,
                    "confidence": item.confidence,
                    "content": item.content,
                })
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn to_prose(&self, result: &ContextResult) -> String {
        if result.items.is_empty() {
            return "No relevant context was found for this query.".to_string();
        }
        let mut out = String::new();
        for item in &result.items {
            out.push_str(&format!(
                "{}: {}\n",
                item.label, item.content
            ));
        }
        out
    }

    /// Format a Table entity into a compact Markdown block.
    /// This is a static utility method used by describe_table.
    pub fn format_table(table: &Table, columns: &[Column], definitions: &[SemanticDefinition]) -> String {
        let mut out = format!("**Table:** `{}`\n", table.name);
        if let Some(desc) = &table.description {
            out.push_str(&format!("**Description:** {desc}\n"));
        }
        if let Some(owner) = &table.owner {
            out.push_str(&format!("**Owner:** {owner}\n"));
        }
        if let Some(rows) = table.row_count {
            out.push_str(&format!("**Rows:** {rows}\n"));
        }
        out.push_str(&format!("**Confidence:** {:.2}\n", table.confidence));

        if !definitions.is_empty() {
            out.push_str("\n**Semantic Definitions:**\n");
            for def in definitions {
                out.push_str(&format!("- {}\n", def.definition));
            }
        }

        if !columns.is_empty() {
            out.push_str("\n**Columns:**\n");
            out.push_str("| Name | Type | Description |\n|------|------|-------------|\n");
            for col in columns {
                let desc = col.description.as_deref().unwrap_or("");
                out.push_str(&format!("| `{}` | {} | {} |\n", col.name, col.data_type, desc));
            }
        }

        out
    }
}

/// Render lineage edges as a compact Markdown DAG section.
fn format_lineage_dag(edges: &[LineageEdge]) -> String {
    if edges.is_empty() {
        return String::new();
    }
    let mut out = String::from("### Lineage\n```\n");
    for edge in edges {
        let short_up = &edge.upstream_id.to_string()[..8];
        let short_down = &edge.downstream_id.to_string()[..8];
        out.push_str(&format!("{short_up}.. → {short_down}..\n"));
    }
    out.push_str("```\n\n");
    out
}
