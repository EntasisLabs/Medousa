use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostContextPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostContextSelection {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<HostContextPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<HostContextPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostContextDiagnostic {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<HostContextPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<HostContextPosition>,
}

/// Bounded, advisory context captured by an editor, notes app, or browser.
/// This never grants filesystem or vault authority to the sending client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct HostTurnContext {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<HostContextPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<HostContextSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<HostContextDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TurnArtifactRef {
    pub role: String,
    pub content_type: String,
    pub byte_size: usize,
    pub hash64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnPart {
    /// Successful inference route for this assistant turn. This records the
    /// daemon-observed route after fallback, not the composer's requested pick.
    ModelReceipt {
        provider: String,
        model: String,
    },
    Text {
        markdown: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segment_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_round: Option<usize>,
    },
    /// Ephemeral-style progress captured between tool rounds (not the final answer).
    Progress {
        markdown: String,
    },
    Reasoning {
        markdown: String,
    },
    ToolRun {
        run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        artifact_refs: Vec<TurnArtifactRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_round: Option<usize>,
        started_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finished_at: Option<DateTime<Utc>>,
    },
    Handoff {
        handoff_kind: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_id: Option<String>,
    },
    UserMedia {
        media_id: String,
        mime: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        byte_size: Option<u64>,
    },
    HostContext {
        context: HostTurnContext,
    },
    AttachmentRef {
        artifact_id: String,
        mime: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        byte_size: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height_px: Option<u32>,
    },
    /// Forward-compatible catch-all for newer clients reading older persisted timelines.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TurnSliceSummary {
    pub goal: String,
    pub tool_rounds: usize,
    pub tools: Vec<String>,
    pub outcomes: Vec<String>,
    pub failures: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch_phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegate_work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegate_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_gaps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_digests: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub working_notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::TurnPart;
    use serde_json::json;

    #[test]
    fn legacy_text_part_defaults_segment_metadata() {
        let part: TurnPart = serde_json::from_value(json!({
            "kind": "text",
            "markdown": "legacy answer"
        }))
        .unwrap();

        assert_eq!(
            part,
            TurnPart::Text {
                markdown: "legacy answer".into(),
                segment_id: None,
                model_round: None,
            }
        );
        assert_eq!(
            serde_json::to_value(part).unwrap(),
            json!({"kind": "text", "markdown": "legacy answer"})
        );
    }

    #[test]
    fn text_part_roundtrips_segment_metadata() {
        let part = TurnPart::Text {
            markdown: "first segment".into(),
            segment_id: Some("segment-1".into()),
            model_round: Some(2),
        };
        let encoded = serde_json::to_value(&part).unwrap();
        assert_eq!(encoded["segment_id"], "segment-1");
        assert_eq!(encoded["model_round"], 2);
        assert_eq!(serde_json::from_value::<TurnPart>(encoded).unwrap(), part);
    }
}
