use anyhow::Result;
use arcana_core::{entities::AgentInteraction, store::MetadataStore};
use std::sync::Arc;
use uuid::Uuid;

/// Records agent interactions and feedback for learning and monitoring.
pub struct FeedbackRecorder {
    store: Arc<dyn MetadataStore>,
}

impl FeedbackRecorder {
    pub fn new(store: Arc<dyn MetadataStore>) -> Self {
        Self { store }
    }

    /// Record that an agent called a tool with the given inputs and referenced these entities.
    pub async fn record_interaction(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        referenced_entity_ids: Vec<Uuid>,
        agent_id: Option<String>,
        latency_ms: Option<i64>,
    ) -> Result<AgentInteraction> {
        let interaction = AgentInteraction {
            id: Uuid::new_v4(),
            tool_name: tool_name.to_string(),
            input,
            referenced_entity_ids,
            agent_id,
            was_helpful: None,
            latency_ms,
            created_at: chrono::Utc::now(),
        };

        self.store.insert_agent_interaction(&interaction).await?;
        Ok(interaction)
    }

    /// Record feedback (thumbs up/down) for a previous interaction.
    pub async fn record_feedback(
        &self,
        interaction_id: Uuid,
        was_helpful: bool,
    ) -> Result<()> {
        self.store
            .update_interaction_feedback(interaction_id, was_helpful)
            .await
    }
}
