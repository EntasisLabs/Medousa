//! Byte-equality: spine fold vs legacy `append_turn` body (Phase 1 contract).

use medousa_engine::{Principal, TurnEnvelope, TurnEvent, TurnEventLog};
use medousa_types::session::ConversationTurn;

use crate::turn_parts::{TurnPart, TurnPartsAccumulator};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "medousa-spine-parity-{tag}-{}",
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn env(turn: &str) -> TurnEnvelope {
        TurnEnvelope::new(turn, Principal::operator())
    }

    fn assert_fold_matches_legacy(tag: &str, legacy: &ConversationTurn, event: TurnEvent) {
        let root = tmp_root(tag);
        let log = TurnEventLog::open_in(&root, env(tag)).unwrap();
        log.append(TurnEvent::ContentDelta {
            delta: "live token".into(),
        });
        log.append(event);
        let folded = log.fold_history();
        assert_eq!(folded.len(), 1, "[{tag}] exactly one history body");
        let spine = &folded[0];

        let legacy_json = serde_json::to_string(legacy).unwrap();
        let spine_json = serde_json::to_string(spine).unwrap();
        assert_eq!(
            spine_json, legacy_json,
            "[{tag}] spine fold body must be byte-identical to legacy append_turn body"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fold_byte_matches_legacy_plain_turn() {
        let mut acc = TurnPartsAccumulator::default();
        let legacy = acc.finalize_assistant_turn(
            "Here is the full explanation with no tool work.".into(),
            vec![],
            None,
        );
        let event = TurnEvent::final_response_from_turn(&legacy);
        assert_fold_matches_legacy("be-plain", &legacy, event);
    }

    #[test]
    fn fold_byte_matches_legacy_tool_turn() {
        let mut acc = TurnPartsAccumulator::default();
        acc.tool_started("tr-1", "data_probe", "q=ingest", 1);
        acc.tool_finished("tr-1", "succeeded", Some("3 hits".into()), vec![]);
        acc.push_reasoning_delta("weighing the probe output");
        let legacy = acc.finalize_assistant_turn(
            "Final answer grounded in the probe.".into(),
            vec!["data_probe".into()],
            None,
        );
        let event = TurnEvent::final_response_from_turn(&legacy);
        assert_fold_matches_legacy("be-tool", &legacy, event);
    }

    #[test]
    fn fold_byte_matches_legacy_interim_prose_turn() {
        let mut acc = TurnPartsAccumulator::default();
        acc.archive_progress_note("Let me check that for you.");
        acc.tool_started("tr-1", "data_probe", "q=ingest", 1);
        acc.tool_finished("tr-1", "succeeded", None, vec![]);
        let legacy = acc.finalize_assistant_turn(
            "Done — here is the result.".into(),
            vec!["data_probe".into()],
            None,
        );
        let event = TurnEvent::final_response_from_turn(&legacy);
        assert_fold_matches_legacy("be-interim", &legacy, event);
    }

    #[test]
    fn fold_byte_matches_legacy_checkpoint_turn() {
        let mut acc = TurnPartsAccumulator::default();
        acc.push_attachment_ref("art:1", "text/html", "Chart", Some(1200), None, None);
        let legacy = acc.finalize_assistant_turn(
            "Found three blockers — your call on scope.".into(),
            vec![],
            Some("checkpoint".to_string()),
        );
        let event = TurnEvent::checkpoint_from_turn(&legacy);
        assert_fold_matches_legacy("be-checkpoint", &legacy, event);
    }

    #[test]
    fn fold_byte_matches_legacy_needs_input_turn() {
        let mut acc = TurnPartsAccumulator::default();
        let legacy = acc.finalize_assistant_turn(
            "Which environment should I target?".into(),
            vec![],
            Some("needs_input".to_string()),
        );
        let event = TurnEvent::needs_input_from_turn(&legacy);
        assert_fold_matches_legacy("be-needs-input", &legacy, event);
    }

    #[test]
    fn fold_byte_matches_legacy_worker_ack_turn() {
        let mut acc = TurnPartsAccumulator::default();
        let legacy = acc.finalize_worker_ack_turn(
            "On it — spawned a background worker.".into(),
            vec!["spawn".into()],
            Some("work-1".into()),
        );
        assert!(legacy
            .parts
            .as_ref()
            .unwrap()
            .iter()
            .any(|p| matches!(p, TurnPart::Handoff { .. })));
        let event = TurnEvent::worker_ack_from_turn(&legacy, Some("work-1".into()));
        assert_fold_matches_legacy("be-worker-ack", &legacy, event);
    }
}
