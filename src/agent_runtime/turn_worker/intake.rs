//! A status read is not delivery: acknowledge fully read results only after the
//! active host has persisted a terminal response. This never approves Forge work.
use super::store::{TurnWorkDisposition, TurnWorkRecord, TurnWorkStatus, turn_worker_store};
use std::{collections::HashMap, sync::Mutex};
pub(crate) const RESULT_PAGE_CHARS: usize = 1200;

#[derive(Default)]
pub(super) struct ObservedResults(Mutex<HashMap<String, ObservedResult>>);
struct ObservedResult {
    record: TurnWorkRecord,
    ranges: Vec<(usize, usize)>,
}
impl ObservedResults {
    fn observe(&self, record: &TurnWorkRecord, offset: usize) {
        if record.disposition != TurnWorkDisposition::Parallel
            || record.synthesis_delivered
            || matches!(
                record.status,
                TurnWorkStatus::Pending | TurnWorkStatus::Running
            )
        {
            return;
        }
        let total = record
            .result_text
            .as_deref()
            .map_or(0, |text| text.chars().count());
        if offset > total {
            return;
        }
        let mut results = self.0.lock().expect("observed worker results");
        let result = results
            .entry(record.work_id.clone())
            .or_insert_with(|| ObservedResult {
                record: record.clone(),
                ranges: Vec::new(),
            });
        if result.record.updated_at != record.updated_at {
            result.record = record.clone();
            result.ranges.clear();
        }
        result
            .ranges
            .push((offset, offset.saturating_add(RESULT_PAGE_CHARS).min(total)));
    }
    fn complete_records(&self, session_id: &str) -> Vec<TurnWorkRecord> {
        self.0
            .lock()
            .expect("observed worker results")
            .values()
            .filter_map(|result| {
                if result.record.session_id != session_id {
                    return None;
                }
                let total = result
                    .record
                    .result_text
                    .as_deref()
                    .map_or(0, |text| text.chars().count());
                let mut ranges = result.ranges.clone();
                ranges.sort_unstable();
                let mut end = 0;
                for (start, next) in ranges {
                    if start > end {
                        return None;
                    }
                    end = end.max(next);
                }
                (end == total).then(|| result.record.clone())
            })
            .collect()
    }
}
tokio::task_local! { pub(super) static OBSERVED_RESULTS: ObservedResults; }
pub(crate) fn observe_worker_result_page(
    record: &TurnWorkRecord,
    offset: usize,
    host_session: &str,
) {
    if record.session_id == host_session {
        let _ = OBSERVED_RESULTS.try_with(|observed| observed.observe(record, offset));
    }
}
/// Call only after successful persistence of a final or needs-input response.
pub async fn acknowledge_observed_worker_results(session_id: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let records = OBSERVED_RESULTS
        .try_with(|observed| observed.complete_records(session_id))
        .unwrap_or_default();
    if records.is_empty() {
        return;
    }
    let store = turn_worker_store();
    for observed in records {
        if let Err(error) = store.try_update_when(&observed.work_id, |current| {
            if current.updated_at == observed.updated_at
                && current.session_id == session_id
                && current.disposition == TurnWorkDisposition::Parallel
                && current.status == observed.status
                && current.result_text == observed.result_text
            {
                current.synthesis_delivered = true;
                true
            } else {
                false
            }
        }) {
            tracing::warn!(work_id = %observed.work_id, %error, "host worker intake persistence failed");
        }
    }
    if let Err(error) = crate::workspace::flush_persist_writer().await {
        tracing::warn!(%error, "host worker intake flush failed");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record() -> TurnWorkRecord {
        serde_json::from_value(serde_json::json!({
            "work_id":"worker-1", "session_id":"session-1", "intent":"coder",
            "task_prompt":"Review", "status":"completed", "tool_names":[],
            "user_ack":"reviewing", "provider":"test", "model":"test",
            "response_depth_mode":"standard", "result_text":"é🦀".repeat(1300),
            "created_at":"2026-09-08T00:00:00Z", "updated_at":"2026-09-08T00:00:00Z"
        }))
        .unwrap()
    }
    #[test]
    fn intake_requires_all_pages_of_one_result_and_same_session() {
        let observed = ObservedResults::default();
        let record = record();
        observed.observe(&record, 2400);
        assert!(observed.complete_records("session-1").is_empty());
        observed.observe(&record, 0);
        assert!(observed.complete_records("session-1").is_empty());
        observed.observe(&record, 1200);
        assert_eq!(observed.complete_records("session-1").len(), 1);
        assert!(observed.complete_records("other-session").is_empty());
        assert!(!record.synthesis_delivered, "read alone is not delivery");
    }
    #[test]
    fn new_version_resets_pages_and_nonterminal_or_delegated_work_is_not_intake() {
        let observed = ObservedResults::default();
        let mut record = record();
        observed.observe(&record, 0);
        record.updated_at += chrono::Duration::seconds(1);
        observed.observe(&record, 1200);
        observed.observe(&record, 2400);
        assert!(observed.complete_records("session-1").is_empty());
        observed.observe(&record, 0);
        assert_eq!(observed.complete_records("session-1").len(), 1);
        for status in [TurnWorkStatus::Pending, TurnWorkStatus::Running] {
            let fresh = ObservedResults::default();
            record.status = status;
            fresh.observe(&record, 0);
            assert!(fresh.complete_records("session-1").is_empty());
        }
        let fresh = ObservedResults::default();
        record.status = TurnWorkStatus::Completed;
        record.disposition = TurnWorkDisposition::Delegated;
        fresh.observe(&record, 0);
        assert!(fresh.complete_records("session-1").is_empty());
    }
}
