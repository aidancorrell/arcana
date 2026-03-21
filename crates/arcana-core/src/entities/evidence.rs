use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Tracks evidence that a semantic definition was used in a successful or failed query.
/// Used to adjust confidence scores over time based on real-world usage.
#[derive(Debug, Clone)]
pub struct EvidenceRecord {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub interaction_id: Option<Uuid>,
    pub query_text: Option<String>,
    pub outcome: EvidenceOutcome,
    pub source: EvidenceSource,
    pub confidence_delta: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceOutcome {
    Success,
    Failure,
}

impl EvidenceOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceSource {
    AgentFeedback,
    QueryHistory,
    CoOccurrence,
}

impl EvidenceSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentFeedback => "agent_feedback",
            Self::QueryHistory => "query_history",
            Self::CoOccurrence => "co_occurrence",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "agent_feedback" => Some(Self::AgentFeedback),
            "query_history" => Some(Self::QueryHistory),
            "co_occurrence" => Some(Self::CoOccurrence),
            _ => None,
        }
    }
}
