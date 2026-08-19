//! Ephemeral model-facing tool observation budgets.
//!
//! Authoritative tool outputs remain unchanged for execution receipts and UI
//! delivery. This module only compiles the copy placed back into the model's
//! current tool loop, and never persists payloads.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub const PERCEPTION_ROUND_MAX_CHARS: usize = 96 * 1024;
pub const PERCEPTION_CONTEXT_RESERVE_CHARS: usize = 24 * 1024;
pub const PERCEPTION_TOOL_RESULTS_MAX_CHARS: usize =
    PERCEPTION_ROUND_MAX_CHARS - PERCEPTION_CONTEXT_RESERVE_CHARS;
pub const PERCEPTION_RESULT_MAX_CHARS: usize = 48 * 1024;

const MIN_ACTIONABLE_RESULT_CHARS: usize = 1_024;
const PRIORITY_FIELDS: &[&str] = &[
    "ok",
    "error",
    "hint",
    "recoverable",
    "read_status",
    "path",
    "root",
    "bytes",
    "total_lines",
    "digest",
    "coverage",
    "orientation",
    "encoding",
    "status",
    "exit_code",
    "artifact_id",
    "reference",
    "next",
];

#[derive(Default)]
pub struct ToolPerceptionGovernor {
    failure_occurrences: HashMap<String, usize>,
    round_metrics: PerceptionMetricsSnapshot,
    evidence_undertaking_id: Option<String>,
    evidence_store: Option<super::coder_evidence::CoderEvidenceStore>,
    compact_receipt_sink: Option<Arc<dyn super::coder_evidence::CompactEvidenceReceiptSink>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PerceptionMetricsSnapshot {
    pub observed_results: u64,
    pub raw_result_chars: u64,
    pub model_result_chars: u64,
    pub bounded_results: u64,
    pub bounded_requeryable_results: u64,
    pub bounded_replayable_results: u64,
    pub would_spool_results: u64,
    pub would_spool_bytes: u64,
    pub stored_evidence_results: u64,
    pub stored_evidence_logical_bytes: u64,
    pub evidence_store_failures: u64,
    pub evidence_receipt_stage_failures: u64,
    pub failure_clusters: u64,
    pub bounded_round_contexts: u64,
    pub omitted_context_chars: u64,
}

impl PerceptionMetricsSnapshot {
    pub fn has_governor_activity(&self) -> bool {
        self.bounded_results > 0 || self.failure_clusters > 0 || self.bounded_round_contexts > 0
    }

    pub fn telemetry_line(&self, round: usize) -> String {
        format!(
            "◈ perception_governor round={round} observed={} raw_chars={} model_chars={} bounded={} requeryable={} replayable={} would_spool={} would_spool_bytes={} stored_evidence={} stored_evidence_bytes={} evidence_store_failures={} evidence_receipt_stage_failures={} failure_clusters={} bounded_contexts={} omitted_context_chars={}",
            self.observed_results,
            self.raw_result_chars,
            self.model_result_chars,
            self.bounded_results,
            self.bounded_requeryable_results,
            self.bounded_replayable_results,
            self.would_spool_results,
            self.would_spool_bytes,
            self.stored_evidence_results,
            self.stored_evidence_logical_bytes,
            self.evidence_store_failures,
            self.evidence_receipt_stage_failures,
            self.failure_clusters,
            self.bounded_round_contexts,
            self.omitted_context_chars,
        )
    }
}

#[derive(Debug, Default)]
struct PerceptionMetrics {
    observed_results: AtomicU64,
    raw_result_chars: AtomicU64,
    model_result_chars: AtomicU64,
    bounded_results: AtomicU64,
    bounded_requeryable_results: AtomicU64,
    bounded_replayable_results: AtomicU64,
    would_spool_results: AtomicU64,
    would_spool_bytes: AtomicU64,
    stored_evidence_results: AtomicU64,
    stored_evidence_logical_bytes: AtomicU64,
    evidence_store_failures: AtomicU64,
    evidence_receipt_stage_failures: AtomicU64,
    failure_clusters: AtomicU64,
    bounded_round_contexts: AtomicU64,
    omitted_context_chars: AtomicU64,
}

static PERCEPTION_METRICS: LazyLock<PerceptionMetrics> = LazyLock::new(PerceptionMetrics::default);

pub fn perception_metrics_snapshot() -> PerceptionMetricsSnapshot {
    PerceptionMetricsSnapshot {
        observed_results: PERCEPTION_METRICS.observed_results.load(Ordering::Relaxed),
        raw_result_chars: PERCEPTION_METRICS.raw_result_chars.load(Ordering::Relaxed),
        model_result_chars: PERCEPTION_METRICS
            .model_result_chars
            .load(Ordering::Relaxed),
        bounded_results: PERCEPTION_METRICS.bounded_results.load(Ordering::Relaxed),
        bounded_requeryable_results: PERCEPTION_METRICS
            .bounded_requeryable_results
            .load(Ordering::Relaxed),
        bounded_replayable_results: PERCEPTION_METRICS
            .bounded_replayable_results
            .load(Ordering::Relaxed),
        would_spool_results: PERCEPTION_METRICS
            .would_spool_results
            .load(Ordering::Relaxed),
        would_spool_bytes: PERCEPTION_METRICS.would_spool_bytes.load(Ordering::Relaxed),
        stored_evidence_results: PERCEPTION_METRICS
            .stored_evidence_results
            .load(Ordering::Relaxed),
        stored_evidence_logical_bytes: PERCEPTION_METRICS
            .stored_evidence_logical_bytes
            .load(Ordering::Relaxed),
        evidence_store_failures: PERCEPTION_METRICS
            .evidence_store_failures
            .load(Ordering::Relaxed),
        evidence_receipt_stage_failures: PERCEPTION_METRICS
            .evidence_receipt_stage_failures
            .load(Ordering::Relaxed),
        failure_clusters: PERCEPTION_METRICS.failure_clusters.load(Ordering::Relaxed),
        bounded_round_contexts: PERCEPTION_METRICS
            .bounded_round_contexts
            .load(Ordering::Relaxed),
        omitted_context_chars: PERCEPTION_METRICS
            .omitted_context_chars
            .load(Ordering::Relaxed),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvidenceClass {
    Requeryable,
    Replayable,
    NonReplayable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservationKind {
    Passthrough,
    BoundedPayload,
    FailureCluster,
    BatchTooWide,
}

impl ToolPerceptionGovernor {
    pub fn for_coder_undertaking(
        undertaking_id: Option<String>,
        compact_receipt_sink: Option<Arc<dyn super::coder_evidence::CompactEvidenceReceiptSink>>,
    ) -> Self {
        let evidence_store = undertaking_id.as_ref().map(|_| {
            super::coder_evidence::CoderEvidenceStore::for_data_root(
                &crate::paths::medousa_data_dir(),
            )
        });
        Self {
            evidence_undertaking_id: undertaking_id,
            evidence_store,
            compact_receipt_sink,
            ..Default::default()
        }
    }

    /// Allocate one deterministic result ceiling from the fixed round pool.
    /// Equal allocation means parallel completion order cannot change budgets.
    pub fn result_budget_for_batch(&self, result_count: usize) -> usize {
        let result_count = result_count.max(1);
        (PERCEPTION_TOOL_RESULTS_MAX_CHARS / result_count).min(PERCEPTION_RESULT_MAX_CHARS)
    }

    /// Compile one authoritative tool output into its ephemeral model-facing
    /// observation. The input value is never mutated or retained.
    pub fn observe(&mut self, tool_name: &str, output: &Value, max_chars: usize) -> Value {
        self.observe_for_call(tool_name, None, output, max_chars)
    }

    pub fn observe_for_call(
        &mut self,
        tool_name: &str,
        source_call_id: Option<&str>,
        output: &Value,
        max_chars: usize,
    ) -> Value {
        let rendered = output.to_string();
        let raw_chars = rendered.chars().count();
        if max_chars < MIN_ACTIONABLE_RESULT_CHARS {
            let observed = minimal_observation(tool_name, output, max_chars);
            self.record_result(
                tool_name,
                output,
                raw_chars,
                rendered.len(),
                &observed,
                ObservationKind::BatchTooWide,
            );
            return observed;
        }
        if is_failure(output) {
            let signature = failure_signature(tool_name, output);
            let occurrences = self
                .failure_occurrences
                .entry(signature.clone())
                .or_insert(0);
            *occurrences = occurrences.saturating_add(1);
            if *occurrences > 1 {
                let observed =
                    fit_failure_cluster(tool_name, output, &signature, *occurrences, max_chars);
                self.record_result(
                    tool_name,
                    output,
                    raw_chars,
                    rendered.len(),
                    &observed,
                    ObservationKind::FailureCluster,
                );
                return observed;
            }
        }

        if raw_chars <= max_chars {
            let observed = output.clone();
            self.record_result(
                tool_name,
                output,
                raw_chars,
                rendered.len(),
                &observed,
                ObservationKind::Passthrough,
            );
            return observed;
        }
        let raw_bytes = rendered.len();
        let evidence = self.persist_evidence(tool_name, source_call_id, output);
        let observed =
            fit_bounded_observation(tool_name, output, rendered, max_chars, evidence.as_ref());
        self.record_result(
            tool_name,
            output,
            raw_chars,
            raw_bytes,
            &observed,
            ObservationKind::BoundedPayload,
        );
        observed
    }

    fn persist_evidence(
        &mut self,
        tool_name: &str,
        source_call_id: Option<&str>,
        output: &Value,
    ) -> Option<Value> {
        if evidence_class(tool_name, output) != EvidenceClass::NonReplayable
            || !is_evidence_payload(output)
        {
            return None;
        }
        let undertaking_id = self.evidence_undertaking_id.as_deref()?;
        let retention = if is_failure(output) {
            super::coder_evidence::EvidenceRetention::FailedOrNonReproducible
        } else {
            super::coder_evidence::EvidenceRetention::SuccessfulOrReproducible
        };
        let store = self.evidence_store.as_ref()?;
        match store.put(
            undertaking_id,
            output,
            retention,
            super::coder_tools::COGNITION_CODER_EVIDENCE_READ,
        ) {
            Ok(receipt) => {
                let durable_receipt_staged = self.compact_receipt_sink.as_ref().is_some_and(|sink| {
                    match sink.stage_compact_receipt(tool_name, source_call_id, &receipt) {
                        Ok(()) => true,
                        Err(err) => {
                            add_metric(
                                &mut self.round_metrics.evidence_receipt_stage_failures,
                                &PERCEPTION_METRICS.evidence_receipt_stage_failures,
                                1,
                            );
                            tracing::warn!(tool = tool_name, error = %err, "Coder compact evidence receipt was not staged");
                            false
                        }
                    }
                });
                add_metric(
                    &mut self.round_metrics.stored_evidence_results,
                    &PERCEPTION_METRICS.stored_evidence_results,
                    1,
                );
                add_metric(
                    &mut self.round_metrics.stored_evidence_logical_bytes,
                    &PERCEPTION_METRICS.stored_evidence_logical_bytes,
                    receipt.logical_bytes,
                );
                Some(json!({
                    "status": "stored",
                    "receipt": receipt,
                    "durable_receipt_staged": durable_receipt_staged,
                    "next_decision": "Read only the byte range needed with the receipt's read_tool and reference; this evidence is redacted, scoped, and ephemeral.",
                }))
            }
            Err(err) => {
                add_metric(
                    &mut self.round_metrics.evidence_store_failures,
                    &PERCEPTION_METRICS.evidence_store_failures,
                    1,
                );
                tracing::warn!(tool = tool_name, error = %err, "Coder evidence was not persisted");
                Some(json!({
                    "status": "unavailable",
                    "reason": "evidence_store_boundary_rejected_or_unavailable",
                    "next_decision": "Use the bounded head/tail orientation now; narrow or rerun the command if more evidence is required.",
                }))
            }
        }
    }

    /// Hard backstop for mode-owned world refreshes. Providers are expected to
    /// compile focused context themselves; this only prevents an anomalous
    /// refresh from escaping the global round envelope.
    pub fn observe_round_context(&mut self, context: &str) -> String {
        let original_chars = context.chars().count();
        if original_chars <= PERCEPTION_CONTEXT_RESERVE_CHARS {
            return context.to_string();
        }
        let mut preview_chars = PERCEPTION_CONTEXT_RESERVE_CHARS / 3;
        loop {
            let bounded = json!({
                "perception_status": "bounded_round_context",
                "reason": "mode_context_exceeds_round_reserve",
                "original_chars": original_chars,
                "context_limit_chars": PERCEPTION_CONTEXT_RESERVE_CHARS,
                "preview_head": take_chars(context, preview_chars),
                "preview_tail": take_last_chars(context, preview_chars),
                "next_decision": "Use the visible pointers and focused discovery tools to resolve omitted world context; do not request the same broad refresh again.",
            })
            .to_string();
            if bounded.chars().count() <= PERCEPTION_CONTEXT_RESERVE_CHARS {
                let omitted = original_chars.saturating_sub(bounded.chars().count());
                self.round_metrics.bounded_round_contexts =
                    self.round_metrics.bounded_round_contexts.saturating_add(1);
                self.round_metrics.omitted_context_chars = self
                    .round_metrics
                    .omitted_context_chars
                    .saturating_add(to_u64(omitted));
                PERCEPTION_METRICS
                    .bounded_round_contexts
                    .fetch_add(1, Ordering::Relaxed);
                PERCEPTION_METRICS
                    .omitted_context_chars
                    .fetch_add(to_u64(omitted), Ordering::Relaxed);
                return bounded;
            }
            preview_chars /= 2;
        }
    }

    pub fn take_round_metrics(&mut self) -> PerceptionMetricsSnapshot {
        std::mem::take(&mut self.round_metrics)
    }

    fn record_result(
        &mut self,
        tool_name: &str,
        output: &Value,
        raw_chars: usize,
        raw_bytes: usize,
        observed: &Value,
        kind: ObservationKind,
    ) {
        let observed_chars = observed.to_string().chars().count();
        add_metric(
            &mut self.round_metrics.observed_results,
            &PERCEPTION_METRICS.observed_results,
            1,
        );
        add_metric(
            &mut self.round_metrics.raw_result_chars,
            &PERCEPTION_METRICS.raw_result_chars,
            to_u64(raw_chars),
        );
        add_metric(
            &mut self.round_metrics.model_result_chars,
            &PERCEPTION_METRICS.model_result_chars,
            to_u64(observed_chars),
        );

        if kind == ObservationKind::FailureCluster {
            add_metric(
                &mut self.round_metrics.failure_clusters,
                &PERCEPTION_METRICS.failure_clusters,
                1,
            );
        }
        if kind != ObservationKind::BoundedPayload {
            return;
        }

        add_metric(
            &mut self.round_metrics.bounded_results,
            &PERCEPTION_METRICS.bounded_results,
            1,
        );
        match evidence_class(tool_name, output) {
            EvidenceClass::Requeryable => add_metric(
                &mut self.round_metrics.bounded_requeryable_results,
                &PERCEPTION_METRICS.bounded_requeryable_results,
                1,
            ),
            EvidenceClass::Replayable => add_metric(
                &mut self.round_metrics.bounded_replayable_results,
                &PERCEPTION_METRICS.bounded_replayable_results,
                1,
            ),
            EvidenceClass::NonReplayable => {
                add_metric(
                    &mut self.round_metrics.would_spool_results,
                    &PERCEPTION_METRICS.would_spool_results,
                    1,
                );
                add_metric(
                    &mut self.round_metrics.would_spool_bytes,
                    &PERCEPTION_METRICS.would_spool_bytes,
                    to_u64(raw_bytes),
                );
            }
        }
    }
}

fn evidence_class(tool_name: &str, output: &Value) -> EvidenceClass {
    if super::tool_stream::tool_payload_is_requeryable(tool_name) {
        EvidenceClass::Requeryable
    } else if ["artifact_id", "reference", "blob_ref", "resource_uri"]
        .iter()
        .any(|key| output.get(*key).is_some_and(|value| !value.is_null()))
    {
        EvidenceClass::Replayable
    } else {
        EvidenceClass::NonReplayable
    }
}

fn is_evidence_payload(output: &Value) -> bool {
    const EVIDENCE_FIELDS: &[&str] = &[
        "stdout",
        "stderr",
        "log",
        "logs",
        "trace",
        "diagnostics",
        "events",
    ];
    output.as_object().is_some_and(|object| {
        EVIDENCE_FIELDS
            .iter()
            .any(|field| object.get(*field).is_some_and(|value| !value.is_null()))
    })
}

fn add_metric(round: &mut u64, global: &AtomicU64, value: u64) {
    *round = round.saturating_add(value);
    global.fetch_add(value, Ordering::Relaxed);
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn minimal_observation(tool_name: &str, output: &Value, max_chars: usize) -> Value {
    let value = json!({
        "ok": output.get("ok").cloned().unwrap_or(Value::Bool(true)),
        "perception_status": "round_batch_too_wide",
        "tool": tool_name,
        "next_decision": "Reduce the number of tools in the next batch and retry only the focused calls still needed.",
    });
    if value.to_string().chars().count() <= max_chars {
        value
    } else {
        json!({
            "perception_status": "round_batch_too_wide",
            "next_decision": "Use a smaller tool batch."
        })
    }
}

fn is_failure(output: &Value) -> bool {
    matches!(output.get("ok").and_then(Value::as_bool), Some(false))
        || output.get("error").is_some()
}

fn failure_signature(tool_name: &str, output: &Value) -> String {
    let error = output
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("tool returned failure");
    let mut hasher = Sha256::new();
    hasher.update(tool_name.as_bytes());
    hasher.update([0]);
    hasher.update(
        error
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_bytes(),
    );
    let digest = format!("{:x}", hasher.finalize());
    format!("sha256:{}", &digest[..16])
}

fn fit_failure_cluster(
    tool_name: &str,
    output: &Value,
    signature: &str,
    occurrences: usize,
    max_chars: usize,
) -> Value {
    let error = output
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("tool returned failure");
    let hint = output.get("hint").and_then(Value::as_str);
    let mut field_budget = max_chars.saturating_sub(512).max(128) / 2;
    loop {
        let value = json!({
            "ok": false,
            "perception_status": "failure_cluster",
            "tool": tool_name,
            "failure_signature": signature,
            "occurrences_this_turn": occurrences,
            "error": truncate_middle(error, field_budget),
            "hint": hint.map(|value| truncate_middle(value, field_budget)),
            "next_decision": "This failure repeated in the current tool loop. Change the arguments or approach before retrying; use the preserved error and hint as the recovery boundary.",
        });
        if value.to_string().chars().count() <= max_chars || field_budget <= 128 {
            return value;
        }
        field_budget /= 2;
    }
}

fn fit_bounded_observation(
    tool_name: &str,
    output: &Value,
    rendered: String,
    max_chars: usize,
    evidence: Option<&Value>,
) -> Value {
    let original_chars = rendered.chars().count();
    let available_fields = output
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut preview_chars = max_chars.saturating_sub(1_024).max(256) / 2;
    let mut field_chars = (max_chars / PRIORITY_FIELDS.len().max(1)).clamp(128, 4_096);

    loop {
        let preserved = priority_fields(output, field_chars);
        let value = json!({
            "ok": output.get("ok").cloned().unwrap_or(Value::Bool(true)),
            "perception_status": "bounded",
            "tool": tool_name,
            "reason": "tool_result_exceeds_model_context_budget",
            "original_chars": original_chars,
            "result_limit_chars": max_chars,
            "preserved": preserved,
            "available_fields": available_fields,
            "payload_preview": {
                "head": take_chars(&rendered, preview_chars),
                "tail": take_last_chars(&rendered, preview_chars),
            },
            "ephemeral_evidence": evidence,
            "next_decision": next_decision(output),
        });
        if value.to_string().chars().count() <= max_chars {
            return value;
        }
        if preview_chars > 128 {
            preview_chars /= 2;
        } else if field_chars > 128 {
            field_chars /= 2;
        } else {
            return json!({
                "ok": output.get("ok").cloned().unwrap_or(Value::Bool(true)),
                "perception_status": "bounded",
                "tool": tool_name,
                "reason": "tool_result_exceeds_model_context_budget",
                "original_chars": original_chars,
                "result_limit_chars": max_chars,
                "ephemeral_evidence": evidence,
                "next_decision": next_decision(output),
            });
        }
    }
}

fn priority_fields(output: &Value, max_field_chars: usize) -> Value {
    let Some(object) = output.as_object() else {
        return Value::Null;
    };
    let mut preserved = Map::new();
    for key in PRIORITY_FIELDS {
        if let Some(value) = object.get(*key) {
            preserved.insert((*key).to_string(), bound_field(value, max_field_chars));
        }
    }
    Value::Object(preserved)
}

fn bound_field(value: &Value, max_chars: usize) -> Value {
    let rendered = value.to_string();
    let original_chars = rendered.chars().count();
    if original_chars <= max_chars {
        return value.clone();
    }
    json!({
        "perception_status": "bounded_field",
        "original_chars": original_chars,
        "preview": truncate_middle(&rendered, max_chars.saturating_sub(96).max(32)),
    })
}

fn next_decision(output: &Value) -> &'static str {
    if output.get("orientation").is_some() {
        "Use preserved.orientation to make the next focused range or discovery call."
    } else if output.get("artifact_id").is_some() || output.get("reference").is_some() {
        "Follow the preserved artifact/reference with its focused read or search tool."
    } else if output.get("stdout").is_some() || output.get("stderr").is_some() {
        "Use the head/tail preview to choose a narrower command, search, or diagnostic query; do not repeat the same broad output call."
    } else {
        "Use preserved metadata and the head/tail preview to choose a narrower follow-up query instead of repeating the broad call."
    }
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars < 24 {
        return take_chars(value, max_chars);
    }
    let marker = "…[bounded]…";
    let remaining = max_chars.saturating_sub(marker.chars().count());
    let head = remaining / 2;
    let tail = remaining.saturating_sub(head);
    format!(
        "{}{}{}",
        take_chars(value, head),
        marker,
        take_last_chars(value, tail)
    )
}

fn take_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn take_last_chars(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    value
        .chars()
        .skip(count.saturating_sub(max_chars))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingReceiptSink {
        receipts: std::sync::Mutex<Vec<(String, String, String)>>,
    }

    impl super::super::coder_evidence::CompactEvidenceReceiptSink for RecordingReceiptSink {
        fn stage_compact_receipt(
            &self,
            source_tool: &str,
            source_call_id: Option<&str>,
            receipt: &super::super::coder_evidence::CoderEvidenceReceipt,
        ) -> Result<(), String> {
            self.receipts.lock().unwrap().push((
                source_tool.to_owned(),
                source_call_id.unwrap_or_default().to_owned(),
                receipt.digest.clone(),
            ));
            Ok(())
        }
    }

    #[test]
    fn small_observation_is_unchanged() {
        let input = json!({"ok": true, "content": "small"});
        let mut governor = ToolPerceptionGovernor::default();
        assert_eq!(
            governor.observe("cognition_store_read", &input, 4_096),
            input
        );
    }

    #[test]
    fn oversized_observation_preserves_orientation_and_points_forward() {
        let input = json!({
            "ok": true,
            "path": "src/lib.rs",
            "root": "/worktree",
            "coverage": {"line_start": 1, "line_end": 200},
            "orientation": {"next_read": {"line_start": 201, "line_end": 400}},
            "content": "x".repeat(20_000),
        });
        let mut governor = ToolPerceptionGovernor::default();
        let observed = governor.observe("cognition_store_read", &input, 4_096);
        assert_eq!(observed["perception_status"], "bounded");
        assert_eq!(observed["preserved"]["path"], "src/lib.rs");
        assert_eq!(
            observed["preserved"]["orientation"]["next_read"]["line_start"],
            201
        );
        assert!(observed.to_string().chars().count() <= 4_096);
        assert!(
            observed["next_decision"]
                .as_str()
                .is_some_and(|value| value.contains("range"))
        );
    }

    #[test]
    fn repeated_failures_become_a_causal_cluster() {
        let failure = json!({
            "ok": false,
            "error": "compiler exited with status 1",
            "hint": "inspect the first diagnostic",
        });
        let mut governor = ToolPerceptionGovernor::default();
        assert_eq!(
            governor.observe("cognition_shell_session_run", &failure, 4_096),
            failure
        );
        let second = governor.observe("cognition_shell_session_run", &failure, 4_096);
        assert_eq!(second["perception_status"], "failure_cluster");
        assert_eq!(second["occurrences_this_turn"], 2);
        assert!(
            second["failure_signature"]
                .as_str()
                .is_some_and(|value| value.starts_with("sha256:"))
        );
        let metrics = governor.take_round_metrics();
        assert_eq!(metrics.observed_results, 2);
        assert_eq!(metrics.failure_clusters, 1);
    }

    #[test]
    fn batch_allocation_cannot_exceed_round_tool_pool() {
        let governor = ToolPerceptionGovernor::default();
        for count in [1, 2, 3, 8, 32, 128] {
            let budget = governor.result_budget_for_batch(count);
            assert!(budget <= PERCEPTION_RESULT_MAX_CHARS);
            assert!(budget.saturating_mul(count) <= PERCEPTION_TOOL_RESULTS_MAX_CHARS);
            let mut observations = ToolPerceptionGovernor::default();
            let observed = observations.observe(
                "cognition_test",
                &json!({"ok": true, "content": "x".repeat(100_000)}),
                budget,
            );
            assert!(observed.to_string().chars().count() <= budget);
        }
    }

    #[test]
    fn anomalous_mode_context_is_bounded_with_next_step_guidance() {
        let mut governor = ToolPerceptionGovernor::default();
        let context = format!(
            "head-pointer\n{}\ntail-pointer",
            "\"escaped\\context\"".repeat(4_000)
        );
        let observed = governor.observe_round_context(&context);
        assert!(observed.chars().count() <= PERCEPTION_CONTEXT_RESERVE_CHARS);
        let value: Value = serde_json::from_str(&observed).expect("bounded context json");
        assert_eq!(value["perception_status"], "bounded_round_context");
        assert!(
            value["preview_head"]
                .as_str()
                .is_some_and(|text| text.starts_with("head-pointer"))
        );
        assert!(
            value["preview_tail"]
                .as_str()
                .is_some_and(|text| text.ends_with("tail-pointer"))
        );
        assert!(value["next_decision"].as_str().is_some());
        let metrics = governor.take_round_metrics();
        assert_eq!(metrics.bounded_round_contexts, 1);
        assert!(metrics.omitted_context_chars > 0);
    }

    #[test]
    fn would_spool_metrics_count_only_non_replayable_bounded_payloads() {
        let payload = "x".repeat(20_000);
        let mut governor = ToolPerceptionGovernor::default();
        governor.observe(
            "cognition_store_read",
            &json!({"ok": true, "content": payload}),
            4_096,
        );
        governor.observe(
            "cognition_ui_present",
            &json!({"ok": true, "artifact_id": "artifact-1", "content": payload}),
            4_096,
        );
        governor.observe(
            "cognition_shell_session_run",
            &json!({"ok": true, "stdout": payload}),
            4_096,
        );

        let metrics = governor.take_round_metrics();
        assert_eq!(metrics.observed_results, 3);
        assert_eq!(metrics.bounded_results, 3);
        assert_eq!(metrics.bounded_requeryable_results, 1);
        assert_eq!(metrics.bounded_replayable_results, 1);
        assert_eq!(metrics.would_spool_results, 1);
        assert!(metrics.would_spool_bytes >= 20_000);
        let line = metrics.telemetry_line(4);
        assert!(line.starts_with("◈ perception_governor round=4"));
        assert!(line.contains("would_spool=1"));
        assert!(!line.contains(&"x".repeat(100)));
    }

    #[test]
    fn coder_non_replayable_observation_gets_followable_ephemeral_receipt() {
        let temp = tempfile::TempDir::new().unwrap();
        let receipt_sink = Arc::new(RecordingReceiptSink::default());
        let mut governor = ToolPerceptionGovernor {
            evidence_undertaking_id: Some("work-test".into()),
            evidence_store: Some(super::super::coder_evidence::CoderEvidenceStore::new(
                temp.path().join("coder-evidence"),
                super::super::coder_evidence::EvidencePolicy::default(),
            )),
            compact_receipt_sink: Some(receipt_sink.clone()),
            ..Default::default()
        };
        let observed = governor.observe_for_call(
            "cognition_shell_session_run",
            Some("model-call-7"),
            &json!({
                "ok": false,
                "headers": {"authorization": "Bearer secret"},
                "stderr": "compile failure\n".repeat(4_000),
            }),
            4_096,
        );
        let receipt = &observed["ephemeral_evidence"]["receipt"];
        assert_eq!(observed["perception_status"], "bounded");
        assert_eq!(observed["ephemeral_evidence"]["status"], "stored");
        assert_eq!(
            observed["ephemeral_evidence"]["durable_receipt_staged"],
            true
        );
        assert_eq!(receipt["redacted"], true);
        assert_eq!(
            receipt["read_tool"],
            super::super::coder_tools::COGNITION_CODER_EVIDENCE_READ
        );
        assert!(
            receipt["reference"]
                .as_str()
                .is_some_and(|value| value.starts_with("coder-evidence:sha256:"))
        );
        let stored = governor
            .evidence_store
            .as_ref()
            .unwrap()
            .read_range(
                "work-test",
                receipt["reference"].as_str().unwrap(),
                0,
                32 * 1024,
            )
            .unwrap();
        assert!(stored.content.contains("[REDACTED]"));
        assert!(!stored.content.contains("Bearer secret"));
        let staged = receipt_sink.receipts.lock().unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].0, "cognition_shell_session_run");
        assert_eq!(staged[0].1, "model-call-7");
        assert_eq!(staged[0].2, receipt["digest"].as_str().unwrap());
        drop(staged);
        let metrics = governor.take_round_metrics();
        assert_eq!(metrics.stored_evidence_results, 1);
        assert!(metrics.stored_evidence_logical_bytes > 20_000);
    }
}
