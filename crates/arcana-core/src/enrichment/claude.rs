use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{EnrichmentProvider, EnrichmentRequest, EnrichmentResponse};

/// Calls the Anthropic Messages API to generate semantic definitions.
pub struct ClaudeEnrichmentProvider {
    client: Client,
    api_key: String,
    model: String,
    /// Maximum definitions to generate in a single API call.
    batch_size: usize,
}

impl ClaudeEnrichmentProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>, batch_size: usize) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            batch_size,
        }
    }
}

// ---------------------------------------------------------------------------
// Anthropic API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

// ---------------------------------------------------------------------------
// Impl
// ---------------------------------------------------------------------------

#[async_trait]
impl EnrichmentProvider for ClaudeEnrichmentProvider {
    async fn enrich_batch(
        &self,
        requests: &[EnrichmentRequest],
    ) -> Result<Vec<EnrichmentResponse>> {
        if requests.is_empty() {
            return Ok(vec![]);
        }

        let mut all_responses: Vec<EnrichmentResponse> = Vec::with_capacity(requests.len());

        // Process in sub-batches to respect the configured batch_size
        for chunk in requests.chunks(self.batch_size) {
            let mut responses = self.call_api(chunk).await?;
            all_responses.append(&mut responses);
        }

        Ok(all_responses)
    }
}

impl ClaudeEnrichmentProvider {
    async fn call_api(&self, requests: &[EnrichmentRequest]) -> Result<Vec<EnrichmentResponse>> {
        let prompt = build_prompt(requests);

        let body = MessagesRequest {
            model: self.model.clone(),
            max_tokens: 2048,
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Anthropic API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API returned {status}: {body}");
        }

        let api_resp: MessagesResponse = resp
            .json()
            .await
            .context("failed to parse Anthropic API response")?;

        let text = api_resp
            .content
            .into_iter()
            .next()
            .map(|b| b.text)
            .unwrap_or_default();

        parse_definitions(&text, requests.len())
    }
}

/// Build a structured prompt asking Claude to generate exactly one definition
/// per entity, separated by `---` delimiters for easy parsing.
fn build_prompt(requests: &[EnrichmentRequest]) -> String {
    let mut prompt = String::from(
        "You are a data catalog assistant. For each data entity below, write a concise \
        1-3 sentence plain-English description suitable for a business data catalog. \
        Focus on what the entity represents and its business purpose. \
        Do not mention technical implementation details. \
        Separate each description with exactly three dashes on their own line (---).\n\n",
    );

    for (i, req) in requests.iter().enumerate() {
        prompt.push_str(&format!("Entity {}:\n", i + 1));

        if let Some(col) = &req.column_name {
            prompt.push_str(&format!(
                "Column `{}` in table `{}`\n",
                col, req.table_name
            ));
        } else {
            prompt.push_str(&format!("Table `{}`\n", req.table_name));
        }

        if !req.column_names.is_empty() {
            prompt.push_str(&format!("Columns: {}\n", req.column_names.join(", ")));
        }

        if !req.upstream_tables.is_empty() {
            prompt.push_str(&format!(
                "Built from: {}\n",
                req.upstream_tables.join(", ")
            ));
        }

        prompt.push('\n');
    }

    prompt.push_str(
        "Write exactly one description per entity in order, separated by ---\n\
        Do not include entity numbers or labels — just the description text.\n",
    );

    prompt
}

/// Parse Claude's `---`-delimited response into one `EnrichmentResponse` per request.
fn parse_definitions(text: &str, expected: usize) -> Result<Vec<EnrichmentResponse>> {
    let parts: Vec<&str> = text.split("\n---\n").collect();

    let mut responses: Vec<EnrichmentResponse> = parts
        .into_iter()
        .map(|s| EnrichmentResponse {
            definition: s.trim().to_string(),
        })
        .filter(|r| !r.definition.is_empty())
        .collect();

    // Pad with empty strings if Claude returned fewer than expected
    while responses.len() < expected {
        responses.push(EnrichmentResponse {
            definition: String::new(),
        });
    }

    responses.truncate(expected);
    Ok(responses)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_definitions_splits_correctly() {
        let text = "First table description.\n---\nSecond column description.\n---\nThird.";
        let results = parse_definitions(text, 3).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].definition, "First table description.");
        assert_eq!(results[1].definition, "Second column description.");
        assert_eq!(results[2].definition, "Third.");
    }

    #[test]
    fn parse_definitions_pads_short_response() {
        let text = "Only one description.";
        let results = parse_definitions(text, 3).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].definition, "Only one description.");
        assert!(results[1].definition.is_empty());
    }

    #[test]
    fn build_prompt_includes_table_name_and_columns() {
        let req = EnrichmentRequest {
            table_name: "fct_orders".to_string(),
            column_names: vec!["order_id".to_string(), "amount".to_string()],
            upstream_tables: vec!["stg_orders".to_string()],
            column_name: None,
        };
        let prompt = build_prompt(&[req]);
        assert!(prompt.contains("fct_orders"));
        assert!(prompt.contains("order_id, amount"));
        assert!(prompt.contains("stg_orders"));
    }
}
