//! Exact pre-migration prompt footprint.
//!
//! Phase 0 records the current prompt's coarse constituents before the STTP
//! compiler replaces concatenation. The snapshot is deliberately exact: later
//! work can show which policy slice removed or added attention cost instead of
//! comparing impressions.

use serde::{Deserialize, Serialize};

use super::context_usage::{ESTIMATOR_LABEL, chars_to_tokens};
use super::modes::{
    CoderRuntimePhase, ModeExecutionLane, ResolvedAgentMode, resolve_agent_mode,
    system_prompt_for_mode,
};
use super::system_prompt::DEFAULT_SYSTEM_PROMPT;
use super::turn_completion_fsm::TurnCompletionProfile;
use super::turn_worker::{TurnWorkerIntent, system_prompt_for_host_profile, worker_system_prompt};
use crate::daemon_api::AgentModeId;

const BASELINE_FIXTURE: &str = include_str!("testdata/prompt_footprint_pre_sttp.json");

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

fn char_delta(larger: &str, smaller: &str) -> usize {
    larger
        .chars()
        .count()
        .saturating_sub(smaller.chars().count())
}

fn current_baseline() -> PromptFootprintBaseline {
    let general = DEFAULT_SYSTEM_PROMPT;
    let coder_setup_mode = resolve_agent_mode(AgentModeId::Coder).expect("Coder mode");
    let coder_setup = system_prompt_for_mode(general, &coder_setup_mode);
    let coder_work_mode = ResolvedAgentMode {
        id: AgentModeId::Coder,
        contract_revision: "coder-v3",
        execution_lane: ModeExecutionLane::ForegroundWorkshop,
        completion_profile: TurnCompletionProfile::ForegroundPrincipal,
        coder_phase: Some(CoderRuntimePhase::Work),
    };
    let coder_work = system_prompt_for_mode(general, &coder_work_mode);

    let host_runtime = system_prompt_for_host_profile(general, true, false, false, None);
    let host_liquid = system_prompt_for_host_profile(general, true, false, true, None);
    let host_full = system_prompt_for_host_profile(general, true, true, true, None);
    let host_routed = system_prompt_for_host_profile(general, true, false, false, Some("research"));

    let worker_runtime = worker_system_prompt(
        "baseline-session",
        TurnWorkerIntent::General,
        None,
        false,
        false,
    );
    let worker_liquid = worker_system_prompt(
        "baseline-session",
        TurnWorkerIntent::General,
        None,
        false,
        true,
    );
    let worker_full = worker_system_prompt(
        "baseline-session",
        TurnWorkerIntent::General,
        None,
        true,
        true,
    );

    PromptFootprintBaseline {
        estimator: ESTIMATOR_LABEL.to_string(),
        entries: vec![
            entry("general_core", general.chars().count()),
            entry(
                "coder_setup_overlay",
                char_delta(coder_setup.as_ref(), general),
            ),
            entry(
                "coder_work_overlay",
                char_delta(coder_work.as_ref(), general),
            ),
            entry(
                "host_runtime_appendices",
                char_delta(&host_runtime, general),
            ),
            entry(
                "host_liquid_appendix",
                char_delta(&host_liquid, &host_runtime),
            ),
            entry(
                "host_ui_artifact_appendix",
                char_delta(&host_full, &host_liquid),
            ),
            entry(
                "host_route_appendix",
                char_delta(&host_routed, &host_runtime),
            ),
            entry("worker_runtime_prompt", worker_runtime.chars().count()),
            entry(
                "worker_liquid_appendix",
                char_delta(&worker_liquid, &worker_runtime),
            ),
            entry(
                "worker_ui_artifact_appendix",
                char_delta(&worker_full, &worker_liquid),
            ),
        ],
    }
}

#[test]
fn pre_sttp_prompt_footprint_matches_the_recorded_baseline() {
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
        "general_core",
        "coder_setup_overlay",
        "coder_work_overlay",
        "host_runtime_appendices",
        "host_liquid_appendix",
        "host_ui_artifact_appendix",
        "host_route_appendix",
        "worker_runtime_prompt",
        "worker_liquid_appendix",
        "worker_ui_artifact_appendix",
    ] {
        assert!(
            baseline.entries.iter().any(|entry| entry.id == required),
            "missing prompt footprint entry {required}"
        );
    }
}
