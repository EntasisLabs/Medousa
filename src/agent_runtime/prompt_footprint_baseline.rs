//! Exact production prompt footprint after the STTP cutover.
//!
//! The pre-migration fixture remains in testdata as historical evidence. This
//! snapshot locks the exact host/worker slice selections now used for inference.

use serde::{Deserialize, Serialize};

use super::context_usage::{ESTIMATOR_LABEL, chars_to_tokens};
use super::modes::{CoderRuntimePhase, resolve_agent_mode, system_prompt_for_mode};
use super::turn_worker::{
    TurnWorkerIntent, worker_system_prompt, worker_system_prompt_for_parent_mode,
};
use crate::daemon_api::AgentModeId;

const BASELINE_FIXTURE: &str = include_str!("testdata/prompt_footprint_sttp_cutover.json");

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
struct PromptFootprintBaseline {
    estimator: String,
    entries: Vec<PromptFootprintEntry>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
struct PromptFootprintEntry {
    id: String,
    chars: usize,
    tokens_estimate: u32,
}

fn entry(id: &str, chars: usize) -> PromptFootprintEntry {
    PromptFootprintEntry {
        id: id.to_string(),
        chars,
        tokens_estimate: chars_to_tokens(chars),
    }
}

fn current_baseline() -> PromptFootprintBaseline {
    let general_mode = resolve_agent_mode(AgentModeId::General).expect("General mode");
    let general_host = system_prompt_for_mode(&general_mode);
    let coder_setup_mode = resolve_agent_mode(AgentModeId::Coder).expect("Coder mode");
    let coder_setup_host = system_prompt_for_mode(&coder_setup_mode);
    let mut coder_work_mode = coder_setup_mode;
    coder_work_mode.coder_phase = Some(CoderRuntimePhase::Work);
    let coder_work_host = system_prompt_for_mode(&coder_work_mode);

    let worker_runtime = worker_system_prompt(
        "baseline-session",
        TurnWorkerIntent::General,
        None,
        false,
        false,
    );
    let coder_worker = worker_system_prompt_for_parent_mode(
        "baseline-session",
        TurnWorkerIntent::General,
        None,
        false,
        false,
        Some("coder"),
    );

    PromptFootprintBaseline {
        estimator: ESTIMATOR_LABEL.to_string(),
        entries: vec![
            entry("general_host_policy", general_host.chars().count()),
            entry("coder_setup_host_policy", coder_setup_host.chars().count()),
            entry("coder_work_host_policy", coder_work_host.chars().count()),
            entry("general_worker_policy_hud", worker_runtime.chars().count()),
            entry("coder_worker_policy_hud", coder_worker.chars().count()),
        ],
    }
}

#[test]
fn sttp_cutover_prompt_footprint_matches_the_recorded_baseline() {
    let expected: PromptFootprintBaseline =
        serde_json::from_str(BASELINE_FIXTURE).expect("parse prompt footprint baseline");
    let actual = current_baseline();
    assert_eq!(
        actual,
        expected,
        "prompt footprint drifted; current baseline:\n{}",
        serde_json::to_string_pretty(&actual).expect("serialize current prompt baseline")
    );
}

#[test]
fn baseline_covers_the_current_policy_and_presentation_sources() {
    let baseline = current_baseline();
    for required in [
        "general_host_policy",
        "coder_setup_host_policy",
        "coder_work_host_policy",
        "general_worker_policy_hud",
        "coder_worker_policy_hud",
    ] {
        assert!(
            baseline.entries.iter().any(|entry| entry.id == required),
            "missing prompt footprint entry {required}"
        );
    }
}
