//! Exact production prompt footprint after the STTP cutover.
//!
//! The pre-migration fixture remains in testdata as historical evidence. This
//! snapshot locks the exact host/worker slice selections now used for inference.

use serde::{Deserialize, Serialize};

use super::context_usage::{ESTIMATOR_LABEL, chars_to_tokens};
use super::modes::{CoderRuntimePhase, resolve_agent_mode, system_prompt_for_mode};
use super::prompt_policy::{
    CompiledSttpPolicy, SttpPolicyActor, SttpPolicyMode, SttpPolicySelection, compile_sttp_policy,
};
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    constituents: Vec<PromptFootprintConstituent>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
struct PromptFootprintConstituent {
    id: String,
    chars: usize,
    tokens_estimate: u32,
}

fn compiled_policy_entry(
    id: &str,
    total_chars: usize,
    policy: &CompiledSttpPolicy,
    trailing_component: Option<&str>,
) -> PromptFootprintEntry {
    let mut constituents = Vec::with_capacity(policy.footprint.slices.len() + 2);
    constituents.push(PromptFootprintConstituent {
        id: "sttp_envelope".to_string(),
        chars: policy.footprint.envelope_chars,
        tokens_estimate: policy.footprint.envelope_tokens_estimate,
    });
    constituents.extend(
        policy
            .footprint
            .slices
            .iter()
            .map(|slice| PromptFootprintConstituent {
                id: slice.id.to_string(),
                chars: slice.chars,
                tokens_estimate: slice.tokens_estimate,
            }),
    );
    if let Some(component_id) = trailing_component {
        let trailing_chars = total_chars
            .checked_sub(policy.footprint.total_chars)
            .expect("compiled policy cannot exceed the complete prompt");
        constituents.push(PromptFootprintConstituent {
            id: component_id.to_string(),
            chars: trailing_chars,
            tokens_estimate: chars_to_tokens(trailing_chars),
        });
    }
    assert_eq!(
        constituents
            .iter()
            .map(|constituent| constituent.chars)
            .sum::<usize>(),
        total_chars,
        "constituent footprint must cover {id}"
    );
    PromptFootprintEntry {
        id: id.to_string(),
        chars: total_chars,
        tokens_estimate: chars_to_tokens(total_chars),
        constituents,
    }
}

fn current_baseline() -> PromptFootprintBaseline {
    let general_mode = resolve_agent_mode(AgentModeId::General).expect("General mode");
    let general_host = system_prompt_for_mode(&general_mode);
    let general_host_policy = compile_sttp_policy(SttpPolicySelection::new(
        SttpPolicyMode::General,
        SttpPolicyActor::Host,
    ))
    .expect("General host policy");
    let coder_setup_mode = resolve_agent_mode(AgentModeId::Coder).expect("Coder mode");
    let coder_setup_host = system_prompt_for_mode(&coder_setup_mode);
    let coder_setup_host_policy = compile_sttp_policy(SttpPolicySelection::new(
        SttpPolicyMode::CoderSetup,
        SttpPolicyActor::Host,
    ))
    .expect("Coder setup host policy");
    let mut coder_work_mode = coder_setup_mode;
    coder_work_mode.coder_phase = Some(CoderRuntimePhase::Work);
    let coder_work_host = system_prompt_for_mode(&coder_work_mode);
    let coder_work_host_policy = compile_sttp_policy(SttpPolicySelection::new(
        SttpPolicyMode::CoderWork,
        SttpPolicyActor::Host,
    ))
    .expect("Coder work host policy");

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
    let general_worker_policy = compile_sttp_policy(SttpPolicySelection::new(
        SttpPolicyMode::General,
        SttpPolicyActor::Worker,
    ))
    .expect("General worker policy");
    let coder_worker_policy = compile_sttp_policy(SttpPolicySelection::new(
        SttpPolicyMode::CoderWork,
        SttpPolicyActor::Worker,
    ))
    .expect("Coder worker policy");

    PromptFootprintBaseline {
        estimator: ESTIMATOR_LABEL.to_string(),
        entries: vec![
            compiled_policy_entry(
                "general_host_policy",
                general_host.chars().count(),
                &general_host_policy,
                None,
            ),
            compiled_policy_entry(
                "coder_setup_host_policy",
                coder_setup_host.chars().count(),
                &coder_setup_host_policy,
                None,
            ),
            compiled_policy_entry(
                "coder_work_host_policy",
                coder_work_host.chars().count(),
                &coder_work_host_policy,
                None,
            ),
            compiled_policy_entry(
                "general_worker_policy_hud",
                worker_runtime.chars().count(),
                &general_worker_policy,
                Some("worker_hud"),
            ),
            compiled_policy_entry(
                "coder_worker_policy_hud",
                coder_worker.chars().count(),
                &coder_worker_policy,
                Some("worker_hud"),
            ),
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
