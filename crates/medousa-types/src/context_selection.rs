//! Selection metadata only. Domain owners retain identity, storage and admission.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "tool", content = "arguments")]
pub enum PointerRetrieval {
    /// Existing chat-history read tool; admission still checks session visibility.
    #[serde(rename = "cognition_chat_history_read")]
    ChatHistory { session_id: String },
    /// Existing context follow tool (component/work-card source projection).
    #[serde(rename = "cognition_context_follow_pointer")]
    ContextFollow { pointer_id: String },
    /// Existing engineering pointer follow tool, bound to the admitted undertaking.
    #[serde(rename = "cognition_engineering_pointer_follow")]
    EngineeringFollow { pointer_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PointerRepresentation {
    Breadcrumb,
    Deferred,
}

/// Absence of authoritative relationships means unknown, NOT unresolved. Tool
/// lifecycle success/failure must never manufacture a work relationship.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PointerResolution {
    Unknown,
    Resolved { source: String },
    Superseded { source: String, by: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PointerSelection {
    pub provenance: String,
    pub scope: String,
    /// Independent integer features (0..1000), never evidence confidence.
    pub relevance: u16,
    pub recency: u16,
    pub activity: u16,
    pub reasons: Vec<String>,
    pub representation: PointerRepresentation,
    pub retrieval: PointerRetrieval,
    pub resolution: PointerResolution,
}

impl PointerSelection {
    pub fn rank(&self) -> u32 {
        u32::from(self.relevance.min(1000)) * 4
            + u32::from(self.activity.min(1000)) * 2
            + u32::from(self.recency.min(1000))
    }

    /// Total ordering shared by real producers, independent of input/hash order.
    pub fn compare(&self, id: &str, other: &Self, other_id: &str) -> std::cmp::Ordering {
        other
            .rank()
            .cmp(&self.rank())
            .then_with(|| id.cmp(other_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_native_selection_has_no_confidence_or_work_outcome_heuristic() {
        let selection = PointerSelection {
            provenance: "chat_catalog".into(),
            scope: "profile".into(),
            relevance: 800,
            recency: 200,
            activity: 100,
            reasons: vec!["work_link".into()],
            representation: PointerRepresentation::Deferred,
            retrieval: PointerRetrieval::ChatHistory {
                session_id: "native-session".into(),
            },
            resolution: PointerResolution::Unknown,
        };
        assert_eq!(
            selection.compare("a", &selection, "b"),
            std::cmp::Ordering::Less
        );
        let decoded: PointerSelection =
            serde_json::from_str(&serde_json::to_string(&selection).unwrap()).unwrap();
        assert_eq!(decoded, selection);
        assert_eq!(decoded.resolution, PointerResolution::Unknown);
        let native = serde_json::to_value(&decoded.retrieval).unwrap();
        assert_eq!(native["tool"], "cognition_chat_history_read");
        assert_eq!(native["arguments"]["session_id"], "native-session");
    }
}
