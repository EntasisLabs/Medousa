//! Payload-free attribution for tool-loop model requests. Fingerprints describe
//! the GenAI request, not the provider's hidden rendering or cache breakpoints.

use std::time::Instant;

use genai::chat::{ChatRequest, ChatResponse, ContentPart};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::loop_gate::ToolLoopCompletionGate;
use crate::loop_state::{TurnLedgerEventKind, TurnLedgerRecord};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestSection {
    pub sha256: String,
    pub bytes: usize,
}

fn section(value: &impl Serialize) -> RequestSection {
    // Stream into the digest rather than allocating another copy of a large
    // tool result or base64 observation just to measure it.
    struct HashWriter {
        digest: Sha256,
        bytes: usize,
    }
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.digest.update(bytes);
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter {
        digest: Sha256::new(),
        bytes: 0,
    };
    serde_json::to_writer(&mut writer, value).expect("GenAI request section serializes");
    RequestSection {
        sha256: format!("{:x}", writer.digest.finalize()),
        bytes: writer.bytes,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFootprint {
    pub tools: RequestSection,
    pub system: RequestSection,
    pub messages: Vec<RequestSection>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

impl RequestFootprint {
    pub fn new(
        request: &ChatRequest,
        model: Option<String>,
        reasoning_effort: Option<String>,
    ) -> Self {
        Self {
            tools: section(&request.tools),
            system: section(&request.system),
            messages: request.messages.iter().map(section).collect(),
            model,
            reasoning_effort,
        }
    }

    fn compare(&self, previous: &Self) -> RequestChange {
        let matching_messages = self
            .messages
            .iter()
            .zip(&previous.messages)
            .take_while(|(a, b)| a == b)
            .count();
        RequestChange {
            tools_changed: self.tools != previous.tools,
            system_changed: self.system != previous.system,
            model_changed: self.model != previous.model,
            reasoning_effort_changed: self.reasoning_effort != previous.reasoning_effort,
            matching_messages,
            matching_message_bytes: self
                .messages
                .iter()
                .take(matching_messages)
                .map(|m| m.bytes)
                .sum(),
            previous_message_count: previous.messages.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestChange {
    pub tools_changed: bool,
    pub system_changed: bool,
    pub model_changed: bool,
    pub reasoning_effort_changed: bool,
    /// Common message prefix only; not a count of cache-eligible tokens.
    pub matching_messages: usize,
    pub matching_message_bytes: usize,
    pub previous_message_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportedTokens {
    pub input: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    /// Includes reasoning where the adapter normalizes it into completion usage.
    pub output: Option<u64>,
    pub reasoning: Option<u64>,
}

fn count(value: Option<i32>) -> Option<u64> {
    value.and_then(|v| u64::try_from(v).ok())
}

impl ReportedTokens {
    fn from_response(response: &ChatResponse) -> Self {
        let usage = &response.usage;
        let input = usage.prompt_tokens_details.as_ref();
        Self {
            input: count(usage.prompt_tokens),
            cache_read: count(input.and_then(|d| d.cached_tokens)),
            cache_write: count(input.and_then(|d| d.cache_creation_tokens)),
            output: count(usage.completion_tokens),
            reasoning: count(
                usage
                    .completion_tokens_details
                    .as_ref()
                    .and_then(|d| d.reasoning_tokens),
            ),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeneratedSizes {
    /// Character counts are size attribution, never billed token estimates.
    pub assistant_text_chars: usize,
    pub tool_arguments_chars: usize,
    /// Subset of tool arguments: content/find/replace/edits in code.write.
    pub edit_payload_chars: usize,
    pub tool_calls: usize,
    /// Unescaped top-level intent text, counted once per model-issued call.
    #[serde(default)]
    pub intent_chars: Option<usize>,
    /// Requested children, not proof of execution or successful observations.
    #[serde(default)]
    pub requested_batch_operations: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceUsage {
    pub schema_version: u32,
    pub outcome: String,
    pub elapsed_ms: u64,
    pub response_id: Option<String>,
    pub provider_model: Option<String>,
    pub request: RequestFootprint,
    pub change: Option<RequestChange>,
    pub tokens: ReportedTokens,
    pub generated: GeneratedSizes,
}

/// A guard also records cancelled requests, whose billed usage is unknown.
/// The host's existing session ledger supplies storage and deletion semantics.
pub struct InferenceObservation {
    ports: crate::ports::RuntimePorts,
    stream_turn_id: u64,
    round: usize,
    started: Instant,
    usage: Option<InferenceUsage>,
    tools: Vec<String>,
}

impl InferenceObservation {
    pub fn start(
        gate: Option<&ToolLoopCompletionGate<'_>>,
        round: usize,
        request: RequestFootprint,
        previous: Option<&RequestFootprint>,
    ) -> Self {
        let change = previous.map(|p| request.compare(p));
        Self {
            ports: gate.map(|g| g.runtime_ports.clone()).unwrap_or_default(),
            stream_turn_id: gate.map(|g| g.stream_turn_id).unwrap_or(0),
            round,
            started: Instant::now(),
            tools: Vec::new(),
            usage: Some(InferenceUsage {
                schema_version: 2,
                outcome: "interrupted".into(),
                elapsed_ms: 0,
                response_id: None,
                provider_model: None,
                request,
                change,
                tokens: ReportedTokens::default(),
                generated: GeneratedSizes::default(),
            }),
        }
    }

    pub fn complete(&mut self, response: &ChatResponse) {
        let Some(usage) = self.usage.as_mut() else {
            return;
        };
        usage.outcome = "completed".into();
        usage.response_id = response.response_id.clone();
        usage.provider_model = Some(response.provider_model_iden.to_string());
        usage.tokens = ReportedTokens::from_response(response);
        usage.generated.intent_chars = Some(0);
        usage.generated.requested_batch_operations = Some(0);
        for part in response.content.parts() {
            match part {
                ContentPart::Text(text) => {
                    usage.generated.assistant_text_chars += text.chars().count()
                }
                ContentPart::ToolCall(call) => {
                    self.tools.push(call.fn_name.clone());
                    usage.generated.tool_calls += 1;
                    usage.generated.tool_arguments_chars +=
                        call.fn_arguments.to_string().chars().count();
                    if let Some(intent) = call
                        .fn_arguments
                        .get("intent")
                        .and_then(serde_json::Value::as_str)
                    {
                        *usage
                            .generated
                            .intent_chars
                            .as_mut()
                            .expect("completed count") += intent.chars().count();
                    }
                    if call.fn_name == "cognition_coder_read_batch" {
                        *usage
                            .generated
                            .requested_batch_operations
                            .as_mut()
                            .expect("completed count") += call
                            .fn_arguments
                            .get("operations")
                            .and_then(serde_json::Value::as_array)
                            .map_or(0, Vec::len);
                    }
                    if call
                        .fn_arguments
                        .get("action")
                        .and_then(serde_json::Value::as_str)
                        == Some("code.write")
                    {
                        for key in ["content", "find", "replace", "edits"] {
                            if let Some(value) = call.fn_arguments.get(key) {
                                usage.generated.edit_payload_chars +=
                                    value.to_string().chars().count();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.persist();
    }

    pub fn failed(&mut self, malformed: bool) {
        if let Some(usage) = self.usage.as_mut() {
            usage.outcome = if malformed {
                "malformed_tool_json"
            } else {
                "provider_error"
            }
            .into();
        }
        self.persist();
    }

    fn persist(&mut self) {
        let Some(mut usage) = self.usage.take() else {
            return;
        };
        usage.elapsed_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if let Some(sink) = self.ports.ledger_sink() {
            sink.persist(&TurnLedgerRecord {
                execution_id: None,
                parent_turn_id: None,
                timestamp: chrono::Utc::now(),
                stream_turn_id: self.stream_turn_id,
                kind: TurnLedgerEventKind::Inference,
                detail: usage.outcome.clone(),
                tools_invoked: std::mem::take(&mut self.tools),
                missing_tools: Vec::new(),
                rounds_executed: self.round,
                scratch: None,
                active_profile_id: None,
                bot_id: None,
                bot_profile_revision: None,
                inference: Some(usage),
            });
        }
    }
}

impl Drop for InferenceObservation {
    fn drop(&mut self) {
        self.persist();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genai::chat::ChatMessage;

    #[derive(Default)]
    struct Ledger(std::sync::Mutex<Vec<TurnLedgerRecord>>);

    impl crate::ports::TurnLedgerSink for Ledger {
        fn persist(&self, record: &TurnLedgerRecord) {
            self.0.lock().unwrap().push(record.clone());
        }
    }

    #[test]
    fn failed_and_cancelled_requests_have_one_record_and_unknown_usage() {
        let ledger = std::sync::Arc::new(Ledger::default());
        let gate = ToolLoopCompletionGate::new_for_execution(
            42,
            crate::ports::RuntimePorts::new().with_ledger_sink(ledger.clone()),
            4,
        );
        for outcome in ["provider_error", "malformed_tool_json", "interrupted"] {
            let footprint = RequestFootprint::new(&ChatRequest::from_user("private"), None, None);
            let mut observation = InferenceObservation::start(Some(&gate), 1, footprint, None);
            if outcome != "interrupted" {
                observation.failed(outcome == "malformed_tool_json");
            }
            drop(observation);
            let records = ledger.0.lock().unwrap();
            let record = records.last().unwrap();
            assert_eq!(record.stream_turn_id, 42);
            let usage = record.inference.as_ref().unwrap();
            assert_eq!(usage.outcome, outcome);
            assert!(usage.tokens.input.is_none());
            assert!(usage.tokens.output.is_none());
            assert!(usage.tokens.cache_write.is_none());
            assert!(usage.generated.intent_chars.is_none());
            assert!(usage.generated.requested_batch_operations.is_none());
        }
        assert_eq!(ledger.0.lock().unwrap().len(), 3);
    }

    #[test]
    fn detects_append_rewrite_and_catalog_changes_without_payloads() {
        let mut request = ChatRequest::new(vec![ChatMessage::user("secret source")]);
        let old = RequestFootprint::new(&request, Some("model".into()), None);
        request
            .messages
            .push(ChatMessage::assistant("private result"));
        let appended = RequestFootprint::new(&request, Some("model".into()), None);
        assert_eq!(appended.compare(&old).matching_messages, 1);
        request.messages[0] = ChatMessage::user("changed source");
        request.tools = Some(vec![genai::chat::Tool::new("read")]);
        let changed = RequestFootprint::new(&request, Some("model".into()), None);
        assert!(changed.compare(&appended).tools_changed);
        assert_eq!(changed.compare(&appended).matching_messages, 0);
        let serialized = serde_json::to_string(&old).unwrap();
        assert!(!serialized.contains("secret source"));
        assert_eq!(count(Some(0)), Some(0));
        assert_eq!(count(None), None);
        assert_eq!(count(Some(-1)), None);
    }
}
