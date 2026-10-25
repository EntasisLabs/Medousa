//! Native V3 producer for external agent sessions.
//!
//! Agent sessions retain their stream across prompts. Each prompt has its own
//! bounded pipeline actor, continuing the journal sequence after the preceding
//! terminal fence. Provider callbacks supply the segment and tool boundaries;
//! this is deliberately not a reverse projection of historical V2 events.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, Weak};

use medousa_engine::{TurnEventLog, TurnPipelineHandle};
use medousa_types::{InteractiveTurnStreamEvent, TurnCompletionOutcomeV3, TurnStreamEventV3};
use tokio::sync::Mutex;

use crate::turn_pipeline_output::{TurnJournalOutput, daemon_turn_pipeline_budget};
use crate::turn_stream_registry::TurnStreamEntry;

type Owners = Mutex<HashMap<String, Arc<Mutex<AgentStream>>>>;

fn owners() -> &'static Owners {
    static OWNERS: OnceLock<Owners> = OnceLock::new();
    OWNERS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct TextSegment {
    id: String,
    text: String,
}

#[derive(Default)]
struct ToolState {
    name: String,
    input: String,
    finished: bool,
}

struct AgentStream {
    log: Weak<TurnEventLog>,
    pipeline: TurnPipelineHandle,
    sealed: bool,
    segment: Option<TextSegment>,
    committed: Vec<String>,
    ordinal: u64,
    model_round: usize,
    next_text_round: bool,
    last_message: Option<String>,
    message_had_delta: bool,
    tools: HashMap<String, ToolState>,
    tool_names: HashSet<String>,
}

impl AgentStream {
    fn new(entry: &TurnStreamEntry, turn_id: &str) -> Self {
        let fence = entry.log.replay_fence();
        Self {
            log: Arc::downgrade(&entry.log),
            pipeline: TurnPipelineHandle::spawn(
                turn_id,
                fence,
                daemon_turn_pipeline_budget(),
                Arc::new(TurnJournalOutput::new(
                    entry.channel.clone(),
                    entry.log.clone(),
                )),
            ),
            sealed: false,
            segment: None,
            committed: Vec::new(),
            ordinal: fence,
            model_round: 1,
            next_text_round: false,
            last_message: None,
            message_had_delta: false,
            tools: HashMap::new(),
            tool_names: HashSet::new(),
        }
    }

    fn append(&mut self, turn_id: &str, text: String, facts: &mut Vec<TurnStreamEventV3>) {
        if text.is_empty() {
            return;
        }
        if self.segment.is_none() {
            if self.next_text_round {
                self.model_round = self.model_round.saturating_add(1);
                self.next_text_round = false;
            }
            self.ordinal = self.ordinal.saturating_add(1);
            let id = format!("{turn_id}:text:{}", self.ordinal);
            self.segment = Some(TextSegment {
                id: id.clone(),
                text: String::new(),
            });
            facts.push(TurnStreamEventV3::AssistantTextStarted {
                segment_id: id,
                model_round: self.model_round,
            });
        }
        let segment = self.segment.as_mut().expect("segment initialized");
        segment.text.push_str(&text);
        facts.push(TurnStreamEventV3::ContentAppend {
            segment_id: segment.id.clone(),
            text,
        });
    }

    fn commit(&mut self, facts: &mut Vec<TurnStreamEventV3>, advance_round: bool) {
        if advance_round {
            self.next_text_round = true;
        }
        if let Some(segment) = self.segment.take() {
            self.committed.push(segment.text);
            facts.push(TurnStreamEventV3::AssistantTextCommitted {
                segment_id: segment.id,
            });
        }
    }

    fn finish(
        &mut self,
        facts: &mut Vec<TurnStreamEventV3>,
        outcome: TurnCompletionOutcomeV3,
        message: Option<String>,
        debug_message: Option<String>,
    ) {
        self.commit(facts, false);
        let mut names: Vec<_> = self.tool_names.iter().cloned().collect();
        names.sort();
        facts.push(TurnStreamEventV3::TurnCompleted {
            outcome,
            aggregate_text: self.committed.join("\n\n"),
            tool_names: names,
            operator_message: message,
            debug_message,
        });
        self.sealed = true;
    }

    fn facts(
        &mut self,
        event: InteractiveTurnStreamEvent,
    ) -> Result<Vec<TurnStreamEventV3>, String> {
        let mut facts = Vec::new();
        match event.event_type.as_str() {
            "status" => facts.push(TurnStreamEventV3::Status {
                phase: event.phase,
                operator_message: event
                    .operator_message
                    .or_else(|| (!event.message.is_empty()).then_some(event.message)),
                debug_message: event.debug_message,
            }),
            "content_delta" => {
                let text = event.content_delta.unwrap_or_default();
                if !text.is_empty() {
                    self.message_had_delta = true;
                }
                self.append(&event.turn_id, text, &mut facts);
            }
            "assistant_message" => {
                let text = event.final_text.unwrap_or(event.message);
                // A completed provider item repeats its streamed body. Only a
                // provider that supplied no deltas needs a fallback append.
                if !self.message_had_delta
                    && self.segment.is_none()
                    && self.last_message.as_deref() != Some(&text)
                {
                    self.append(&event.turn_id, text.clone(), &mut facts);
                }
                self.commit(&mut facts, false);
                self.last_message = Some(text);
                self.message_had_delta = false;
            }
            "reasoning_delta" => {
                let text = event
                    .reasoning_delta
                    .or(event.content_delta)
                    .unwrap_or(event.message);
                if !text.is_empty() {
                    facts.push(TurnStreamEventV3::ReasoningAppend { text });
                }
            }
            "model_receipt" => facts.push(TurnStreamEventV3::ModelReceipt {
                provider: event
                    .response_provider
                    .ok_or("external model receipt missing provider")?,
                model: event
                    .response_model
                    .ok_or("external model receipt missing model")?,
            }),
            "tool_started" | "tool_finished" => {
                self.commit(&mut facts, true);
                self.last_message = None;
                let id = event
                    .tool_run_id
                    .ok_or("external tool event missing tool_run_id")?;
                let name = event.tool_name.unwrap_or_else(|| "Agent tool".into());
                let round = event.tool_round.unwrap_or(self.model_round);
                if !self.tools.contains_key(&id) {
                    self.tool_names.insert(name.clone());
                    let input = event.tool_input_summary.clone().unwrap_or_default();
                    self.tools.insert(
                        id.clone(),
                        ToolState {
                            name: name.clone(),
                            input: input.clone(),
                            finished: false,
                        },
                    );
                    facts.push(TurnStreamEventV3::ToolStarted {
                        tool_run_id: id.clone(),
                        tool_name: name.clone(),
                        input_summary: input,
                        input_params: event.tool_input_params.clone().unwrap_or_default(),
                        tool_round: round,
                    });
                }
                let tool = self.tools.get_mut(&id).expect("tool initialized");
                if event.event_type == "tool_finished" && !tool.finished {
                    tool.finished = true;
                    facts.push(TurnStreamEventV3::ToolFinished {
                        tool_run_id: id,
                        tool_name: tool.name.clone(),
                        status: event.tool_status.unwrap_or_else(|| "completed".into()),
                        input_summary: tool.input.clone(),
                        input_params: event.tool_input_params.unwrap_or_default(),
                        output_summary: event.tool_output_summary,
                        tool_round: round,
                        artifact_refs: event.tool_artifact_refs.unwrap_or_default(),
                    });
                }
            }
            "permission_request" => {
                self.commit(&mut facts, false);
                facts.push(TurnStreamEventV3::PermissionRequest {
                    request_id: event
                        .permission_request_id
                        .ok_or("external permission event missing request ID")?,
                    message: event.message,
                    agent_session_id: event.agent_session_id,
                    agent_runtime: event.agent_runtime,
                    permission_options: event.permission_options,
                });
            }
            "error" => {
                self.commit(&mut facts, false);
                let message = event.operator_message.unwrap_or(event.message);
                let cancelled = event.phase == "cancelled";
                facts.push(TurnStreamEventV3::Error {
                    operator_message: message.clone(),
                    debug_message: event.debug_message.clone(),
                });
                self.finish(
                    &mut facts,
                    if cancelled {
                        TurnCompletionOutcomeV3::Cancelled
                    } else {
                        TurnCompletionOutcomeV3::Failed
                    },
                    Some(message),
                    event.debug_message,
                );
            }
            "done" => self.finish(&mut facts, TurnCompletionOutcomeV3::Completed, None, None),
            other => {
                return Err(format!(
                    "unsupported external agent producer event '{other}'"
                ));
            }
        }
        Ok(facts)
    }
}

/// Sequence and durably publish known external-agent producer callbacks as V3.
pub(crate) async fn publish(
    entry: &TurnStreamEntry,
    event: InteractiveTurnStreamEvent,
) -> Result<(), String> {
    // A permission wait or tool callback can finish after session cancellation.
    // Its stream is already closed and its producer owner may be released.
    if entry.channel.is_closed() {
        return Ok(());
    }
    if event.turn_id.trim().is_empty() {
        return Err("external agent stream missing turn ID".into());
    }
    let turn_id = event.turn_id.clone();
    let owner = {
        let mut owners = owners().lock().await;
        owners
            .entry(turn_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(AgentStream::new(entry, &turn_id))))
            .clone()
    };
    let mut state = owner.lock().await;
    // Close can race with the initial check while this callback waits behind
    // the cancellation terminal. Never recreate an actor after that fence.
    if entry.channel.is_closed() {
        state.pipeline.cancel();
        let mut owners = owners().lock().await;
        if owners
            .get(&turn_id)
            .is_some_and(|current| Arc::ptr_eq(current, &owner))
        {
            owners.remove(&turn_id);
        }
        return Ok(());
    }
    let same_log = state
        .log
        .upgrade()
        .is_some_and(|log| Arc::ptr_eq(&log, &entry.log));
    let starts_prompt =
        event.event_type == "status" && matches!(event.phase.as_str(), "running" | "accepted");
    if !same_log || (state.sealed && starts_prompt) {
        state.pipeline.cancel();
        *state = AgentStream::new(entry, &turn_id);
    } else if state.sealed {
        // Provider errors and cancellation can race with a final Done. One
        // prompt has exactly one terminal fact; only running starts another.
        return Ok(());
    }
    let facts = state.facts(event)?;
    for fact in facts {
        if let Err(error) = state.pipeline.emit_v3(fact).await {
            state.sealed = true;
            state.pipeline.cancel();
            return Err(error.to_string());
        }
    }
    Ok(())
}

/// Drop native producer ownership after the agent session is removed.
pub(crate) async fn release(turn_id: &str) {
    let owner = { owners().lock().await.remove(turn_id) };
    if let Some(owner) = owner {
        owner.lock().await.pipeline.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_engine::{Principal, TurnEnvelope};
    use medousa_types::AgentPermissionOption;

    fn stream() -> (tempfile::TempDir, TurnStreamEntry, String) {
        let root = tempfile::tempdir().unwrap();
        let id = format!("external-v3-{}", uuid::Uuid::new_v4());
        let entry = TurnStreamEntry {
            log: Arc::new(
                TurnEventLog::open_in(root.path(), TurnEnvelope::new(&id, Principal::operator()))
                    .unwrap(),
            ),
            channel: crate::turn_event_channel::TurnEventChannel::new(64),
        };
        (root, entry, id)
    }

    fn event(id: &str, kind: &str) -> InteractiveTurnStreamEvent {
        serde_json::from_value(serde_json::json!({"turn_id":id,"seq":0,"event_type":kind,"phase":"streaming","message":"","terminal":false,"emitted_at_utc":chrono::Utc::now()})).unwrap()
    }

    async fn emit(entry: &TurnStreamEntry, id: &str, kind: &str, text: &str) {
        let mut value = event(id, kind);
        if kind == "content_delta" {
            value.content_delta = Some(text.into());
        }
        if kind == "assistant_message" {
            value.final_text = Some(text.into());
        }
        if kind == "status" {
            value.phase = text.into();
        }
        if kind == "error" {
            value.message = text.into();
        }
        publish(entry, value).await.unwrap();
    }

    #[tokio::test]
    async fn external_callbacks_publish_live_v3_with_matching_durable_replay() {
        let (_root, entry, id) = stream();
        let mut live = entry.channel.try_subscribe().unwrap();
        emit(&entry, &id, "status", "running").await;
        emit(&entry, &id, "content_delta", "Before tool").await;
        emit(&entry, &id, "assistant_message", "Before tool").await;
        for (kind, status) in [
            ("tool_started", "running"),
            ("tool_started", "running"),
            ("tool_finished", "completed"),
        ] {
            let mut tool = event(&id, kind);
            tool.tool_run_id = Some("tool-1".into());
            tool.tool_name = Some("Read".into());
            tool.tool_status = Some(status.into());
            tool.tool_input_summary = Some("read a file".into());
            tool.tool_output_summary = Some("file content".into());
            publish(&entry, tool).await.unwrap();
        }
        let mut permission = event(&id, "permission_request");
        permission.permission_request_id = Some("approval-1".into());
        permission.permission_options = Some(vec![AgentPermissionOption {
            option_id: "approve-session".into(),
            name: "Allow for session".into(),
            kind: "allow_always".into(),
        }]);
        publish(&entry, permission).await.unwrap();
        emit(&entry, &id, "content_delta", "After tool").await;
        emit(&entry, &id, "assistant_message", "After tool").await;
        emit(&entry, &id, "done", "").await;

        let replay = entry.log.snapshot_since(0);
        let mut native = Vec::new();
        for record in &replay {
            let delivered = tokio::time::timeout(std::time::Duration::from_secs(2), live.recv())
                .await
                .unwrap()
                .unwrap();
            let delivered = delivered
                .v3
                .as_ref()
                .expect("every external event has native V3");
            assert_eq!(delivered.seq, record.seq());
            assert_eq!(
                serde_json::to_value(&delivered.event).unwrap(),
                serde_json::to_value(record.stream_event_v3.as_ref().unwrap()).unwrap()
            );
            native.push(delivered.event.clone());
        }
        assert_eq!(
            native
                .iter()
                .filter(|event| matches!(event, TurnStreamEventV3::ToolStarted { .. }))
                .count(),
            1
        );
        assert!(native.iter().any(|event| matches!(event, TurnStreamEventV3::PermissionRequest { permission_options: Some(options), .. } if options[0].option_id == "approve-session")));
        assert!(
            matches!(native.last(), Some(TurnStreamEventV3::TurnCompleted { outcome: TurnCompletionOutcomeV3::Completed, aggregate_text, .. }) if aggregate_text == "Before tool\n\nAfter tool")
        );
        assert!(entry.log.last_synced_seq() >= entry.log.replay_fence());
        release(&id).await;
    }

    #[tokio::test]
    async fn two_prompts_share_a_monotonic_stream_but_reset_text_and_terminal_state() {
        let (_root, entry, id) = stream();
        emit(&entry, &id, "status", "accepted").await;
        emit(&entry, &id, "status", "running").await;
        emit(&entry, &id, "content_delta", "First").await;
        emit(&entry, &id, "done", "").await;
        let first_fence = entry.log.replay_fence();
        emit(&entry, &id, "done", "duplicate terminal").await;
        assert_eq!(entry.log.replay_fence(), first_fence);
        emit(&entry, &id, "status", "running").await;
        emit(
            &entry,
            &id,
            "assistant_message",
            "Second, final-only adapter",
        )
        .await;
        emit(&entry, &id, "done", "").await;
        let replay = entry.log.snapshot_since(0);
        for (index, record) in replay.iter().enumerate() {
            assert_eq!(record.seq(), index as u64 + 1);
        }
        let completed: Vec<_> = replay
            .iter()
            .filter_map(|record| match record.stream_event_v3.as_ref()? {
                TurnStreamEventV3::TurnCompleted { aggregate_text, .. } => {
                    Some(aggregate_text.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(completed, ["First", "Second, final-only adapter"]);
        let segments: Vec<_> = replay
            .iter()
            .filter_map(|record| match record.stream_event_v3.as_ref()? {
                TurnStreamEventV3::AssistantTextStarted { segment_id, .. } => Some(segment_id),
                _ => None,
            })
            .collect();
        assert_eq!(segments.len(), 2);
        assert_ne!(segments[0], segments[1]);
        assert!(entry.log.replay_fence() > first_fence);
        release(&id).await;
    }

    #[tokio::test]
    async fn errors_commit_partial_text_once_and_a_later_prompt_can_continue() {
        let (_root, entry, id) = stream();
        emit(&entry, &id, "status", "running").await;
        emit(&entry, &id, "content_delta", "Partial").await;
        emit(&entry, &id, "error", "provider disconnected").await;
        emit(&entry, &id, "done", "").await;
        let replay = entry.log.snapshot_since(0);
        assert_eq!(
            replay
                .iter()
                .filter(|record| matches!(
                    record.stream_event_v3,
                    Some(TurnStreamEventV3::TurnCompleted { .. })
                ))
                .count(),
            1
        );
        assert!(
            matches!(replay.last().unwrap().stream_event_v3.as_ref(), Some(TurnStreamEventV3::TurnCompleted { outcome: TurnCompletionOutcomeV3::Failed, aggregate_text, .. }) if aggregate_text == "Partial")
        );
        emit(&entry, &id, "status", "running").await;
        let mut cancelled = event(&id, "error");
        cancelled.phase = "cancelled".into();
        cancelled.message = "Stopped".into();
        publish(&entry, cancelled).await.unwrap();
        assert!(
            matches!(entry.log.snapshot_since(0).last().unwrap().stream_event_v3.as_ref(), Some(TurnStreamEventV3::TurnCompleted { outcome: TurnCompletionOutcomeV3::Cancelled, aggregate_text, .. }) if aggregate_text.is_empty())
        );
        release(&id).await;
    }

    #[tokio::test]
    async fn late_permission_and_tool_callbacks_cannot_resurrect_a_cancelled_stream() {
        let (_root, entry, id) = stream();
        emit(&entry, &id, "status", "running").await;
        emit(&entry, &id, "content_delta", "Partial").await;
        let mut cancelled = event(&id, "error");
        cancelled.phase = "cancelled".into();
        cancelled.message = "Stopped".into();
        publish(&entry, cancelled).await.unwrap();
        entry.channel.mark_closed();
        release(&id).await;
        let fence = entry.log.replay_fence();

        emit(&entry, &id, "status", "permission_resolved").await;
        let mut late_tool = event(&id, "tool_finished");
        late_tool.tool_run_id = Some("late-tool".into());
        late_tool.tool_name = Some("Read".into());
        publish(&entry, late_tool).await.unwrap();
        emit(&entry, &id, "content_delta", "Late output").await;
        emit(&entry, &id, "done", "").await;

        assert_eq!(entry.log.replay_fence(), fence);
        assert!(!owners().lock().await.contains_key(&id));
        assert!(
            matches!(entry.log.snapshot_since(0).last().unwrap().stream_event_v3.as_ref(), Some(TurnStreamEventV3::TurnCompleted { outcome: TurnCompletionOutcomeV3::Cancelled, aggregate_text, .. }) if aggregate_text == "Partial")
        );
    }
}
