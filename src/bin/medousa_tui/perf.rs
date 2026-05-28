use std::time::Instant;

use chrono::Utc;

use super::TuiState;

#[derive(Default)]
pub(crate) struct UiPerfStats {
    pub(crate) frames_rendered: u64,
    pub(crate) last_frame_render_ms: u64,
    pub(crate) last_input_to_paint_ms: u64,
    pub(crate) total_frame_render_ms: u128,
    pub(crate) total_input_to_paint_ms: u128,
    pub(crate) frame_samples: u64,
    pub(crate) coalesced_agent_chunks: u64,
    pub(crate) coalesced_key_events: u64,
    pub(crate) dropped_events: u64,
    pub(crate) worker_queue_depth: u64,
    pub(crate) worker_queue_peak: u64,
}

#[derive(Clone)]
pub(crate) struct PerfSnapshot {
    pub(crate) label: String,
    pub(crate) captured_at: chrono::DateTime<Utc>,
    pub(crate) last_frame_render_ms: u64,
    pub(crate) avg_frame_render_ms: u64,
    pub(crate) last_input_to_paint_ms: u64,
    pub(crate) avg_input_to_paint_ms: u64,
    pub(crate) dropped_events: u64,
    pub(crate) worker_queue_peak: u64,
}

pub(crate) fn mark_ui_activity(state: &mut TuiState) {
    if state.pending_paint_since.is_none() {
        state.pending_paint_since = Some(Instant::now());
    }
}

pub(crate) fn note_frame_rendered(state: &mut TuiState, started_at: Instant) {
    state.perf.frames_rendered = state.perf.frames_rendered.saturating_add(1);
    state.perf.last_frame_render_ms = started_at.elapsed().as_millis() as u64;
    state.perf.total_frame_render_ms = state
        .perf
        .total_frame_render_ms
        .saturating_add(state.perf.last_frame_render_ms as u128);
    state.perf.frame_samples = state.perf.frame_samples.saturating_add(1);
    if let Some(activity_ts) = state.pending_paint_since.take() {
        state.perf.last_input_to_paint_ms = activity_ts.elapsed().as_millis() as u64;
        state.perf.total_input_to_paint_ms = state
            .perf
            .total_input_to_paint_ms
            .saturating_add(state.perf.last_input_to_paint_ms as u128);
    }
}

fn avg_u64(total: u128, samples: u64) -> u64 {
    if samples == 0 {
        0
    } else {
        (total / samples as u128) as u64
    }
}

pub(crate) fn capture_perf_snapshot(state: &TuiState, label: impl Into<String>) -> PerfSnapshot {
    PerfSnapshot {
        label: label.into(),
        captured_at: Utc::now(),
        last_frame_render_ms: state.perf.last_frame_render_ms,
        avg_frame_render_ms: avg_u64(state.perf.total_frame_render_ms, state.perf.frame_samples),
        last_input_to_paint_ms: state.perf.last_input_to_paint_ms,
        avg_input_to_paint_ms: avg_u64(
            state.perf.total_input_to_paint_ms,
            state.perf.frame_samples,
        ),
        dropped_events: state.perf.dropped_events,
        worker_queue_peak: state.perf.worker_queue_peak,
    }
}

pub(crate) fn format_perf_snapshot(snapshot: &PerfSnapshot) -> String {
    format!(
        "label={} at={} | paint(last/avg)={}/{}ms | frame(last/avg)={}/{}ms | dropped={} | worker_q_peak={}",
        snapshot.label,
        snapshot.captured_at.format("%H:%M:%S"),
        snapshot.last_input_to_paint_ms,
        snapshot.avg_input_to_paint_ms,
        snapshot.last_frame_render_ms,
        snapshot.avg_frame_render_ms,
        snapshot.dropped_events,
        snapshot.worker_queue_peak,
    )
}

pub(crate) fn format_perf_delta(current: &PerfSnapshot, baseline: &PerfSnapshot) -> String {
    let paint_avg_delta =
        current.avg_input_to_paint_ms as i64 - baseline.avg_input_to_paint_ms as i64;
    let frame_avg_delta = current.avg_frame_render_ms as i64 - baseline.avg_frame_render_ms as i64;
    let dropped_delta = current.dropped_events as i64 - baseline.dropped_events as i64;
    let queue_peak_delta = current.worker_queue_peak as i64 - baseline.worker_queue_peak as i64;
    format!(
        "delta vs {}: paint_avg={}ms, frame_avg={}ms, dropped={}, worker_q_peak={}",
        baseline.label, paint_avg_delta, frame_avg_delta, dropped_delta, queue_peak_delta
    )
}
