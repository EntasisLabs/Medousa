use crate::stage_routing::StageRoute;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineExecutionLane {
    Interactive,
    Scheduled,
    Heartbeat,
}

impl EngineExecutionLane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Scheduled => "scheduled",
            Self::Heartbeat => "heartbeat",
        }
    }
}

pub fn default_policy_profile_for_lane(lane: EngineExecutionLane) -> &'static str {
    match lane {
        EngineExecutionLane::Interactive => "interactive",
        EngineExecutionLane::Scheduled => "scheduled",
        EngineExecutionLane::Heartbeat => "heartbeat",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneSafetyActionClass {
    InteractiveIngress,
    RecurringRegistration,
    HeartbeatNotificationDispatch,
}

impl LaneSafetyActionClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InteractiveIngress => "interactive_ingress",
            Self::RecurringRegistration => "recurring_registration",
            Self::HeartbeatNotificationDispatch => "heartbeat_notification_dispatch",
        }
    }
}

pub fn lane_allows_action(lane: EngineExecutionLane, action: LaneSafetyActionClass) -> bool {
    match action {
        LaneSafetyActionClass::InteractiveIngress => lane == EngineExecutionLane::Interactive,
        LaneSafetyActionClass::RecurringRegistration => {
            matches!(lane, EngineExecutionLane::Scheduled | EngineExecutionLane::Interactive)
                && !(lane == EngineExecutionLane::Interactive
                    && block_recurring_registration_on_interactive_lane())
        }
        LaneSafetyActionClass::HeartbeatNotificationDispatch => {
            lane == EngineExecutionLane::Scheduled || lane == EngineExecutionLane::Heartbeat
        }
    }
}

/// Opt-out for hardened deployments. Default: interactive chat (Telegram, TUI, etc.) may
/// register cron/recurring jobs so requests like "remind me every day at 5pm" work.
pub fn block_recurring_registration_on_interactive_lane() -> bool {
    std::env::var("MEDOUSA_LANE_SAFETY_BLOCK_RECURRING_ON_INTERACTIVE")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

pub fn lane_accepts_policy_profile(lane: EngineExecutionLane, profile: &str) -> bool {
    let normalized = profile.trim().to_ascii_lowercase();
    !normalized.is_empty() && normalized == default_policy_profile_for_lane(lane)
}

pub fn validate_lane_action(
    lane: EngineExecutionLane,
    action: LaneSafetyActionClass,
) -> Result<(), String> {
    if lane_allows_action(lane, action) {
        return Ok(());
    }

    Err(format!(
        "lane={} is not allowed to perform action={} under current lane safety matrix",
        lane.as_str(),
        action.as_str(),
    ))
}

pub fn validate_lane_policy_profile(
    lane: EngineExecutionLane,
    policy_profile: Option<&str>,
) -> Result<(), String> {
    let Some(profile) = policy_profile else {
        return Ok(());
    };

    if lane_accepts_policy_profile(lane, profile) {
        return Ok(());
    }

    Err(format!(
        "policy_profile={} is not permitted for lane={} (expected={})",
        profile.trim(),
        lane.as_str(),
        default_policy_profile_for_lane(lane),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallReadiness {
    Verified,
    Unverified,
    Missing,
}

impl RecallReadiness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneExecutionBudget {
    pub max_llm_calls_total: usize,
    pub max_tool_loop_calls: usize,
    pub max_prompt_only_calls: usize,
    pub max_classifier_calls: usize,
    pub max_gatekeeper_calls: usize,
    pub max_retries: usize,
    pub max_continuations: usize,
}

pub fn lane_execution_budget(lane: EngineExecutionLane) -> LaneExecutionBudget {
    match lane {
        EngineExecutionLane::Interactive => LaneExecutionBudget {
            // Classifier + tool loop + continuation/retry need separate budget slots.
            max_llm_calls_total: 20,
            max_tool_loop_calls: 10,
            max_prompt_only_calls: 1,
            max_classifier_calls: 1,
            max_gatekeeper_calls: 0,
            max_retries: 1,
            max_continuations: 1,
        },
        EngineExecutionLane::Scheduled => LaneExecutionBudget {
            max_llm_calls_total: 15,
            max_tool_loop_calls: 10,
            max_prompt_only_calls: 1,
            max_classifier_calls: 0,
            max_gatekeeper_calls: 0,
            max_retries: 1,
            max_continuations: 1,
        },
        EngineExecutionLane::Heartbeat => LaneExecutionBudget {
            max_llm_calls_total: 5,
            max_tool_loop_calls: 5,
            max_prompt_only_calls: 1,
            max_classifier_calls: 0,
            max_gatekeeper_calls: 0,
            max_retries: 0,
            max_continuations: 0,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeartbeatLanePolicy {
    pub min_significance: f32,
    pub dead_letter_weight: f32,
    pub failed_weight: f32,
    pub outbox_weight: f32,
    pub activity_weight: f32,
}

pub fn default_heartbeat_lane_policy() -> HeartbeatLanePolicy {
    HeartbeatLanePolicy {
        min_significance: 0.65,
        dead_letter_weight: 0.55,
        failed_weight: 0.25,
        outbox_weight: 0.15,
        activity_weight: 0.05,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatAction {
    Noop,
    Notify,
}

impl HeartbeatAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Noop => "noop",
            Self::Notify => "notify",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeartbeatSignals {
    pub materialized_jobs: usize,
    pub processed_job: bool,
    pub published_events: usize,
    pub failed_jobs: usize,
    pub dead_letter_jobs: usize,
    pub pending_outbox_events: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeartbeatDecision {
    pub action: HeartbeatAction,
    pub significance: f32,
    pub reason: String,
}

pub fn evaluate_heartbeat_significance(
    signals: &HeartbeatSignals,
    policy: HeartbeatLanePolicy,
    prior_dead_letter_jobs: Option<usize>,
) -> HeartbeatDecision {
    let dead_letter_score = (signals.dead_letter_jobs as f32 / 3.0).clamp(0.0, 1.0);
    let failed_score = (signals.failed_jobs as f32 / 8.0).clamp(0.0, 1.0);
    let outbox_score = (signals.pending_outbox_events as f32 / 200.0).clamp(0.0, 1.0);
    let activity_score = if signals.materialized_jobs > 0
        || signals.processed_job
        || signals.published_events > 0
    {
        1.0
    } else {
        0.0
    };

    let significance = (
        dead_letter_score * policy.dead_letter_weight
            + failed_score * policy.failed_weight
            + outbox_score * policy.outbox_weight
            + activity_score * policy.activity_weight
    )
    .clamp(0.0, 1.0);

    if signals.dead_letter_jobs > 0 {
        let increased = prior_dead_letter_jobs
            .map(|prior| signals.dead_letter_jobs > prior)
            .unwrap_or(true);
        let action = if increased {
            HeartbeatAction::Notify
        } else {
            HeartbeatAction::Noop
        };
        let reason = if increased {
            format!(
                "dead_letter_detected dead_letter_jobs={}",
                signals.dead_letter_jobs
            )
        } else {
            format!(
                "dead_letter_static dead_letter_jobs={}",
                signals.dead_letter_jobs
            )
        };
        return HeartbeatDecision {
            action,
            significance,
            reason,
        };
    }

    if significance >= policy.min_significance {
        return HeartbeatDecision {
            action: HeartbeatAction::Notify,
            significance,
            reason: format!(
                "significance_threshold_exceeded significance={:.2} threshold={:.2}",
                significance, policy.min_significance
            ),
        };
    }

    HeartbeatDecision {
        action: HeartbeatAction::Noop,
        significance,
        reason: format!(
            "below_threshold significance={:.2} threshold={:.2}",
            significance, policy.min_significance
        ),
    }
}

#[derive(Debug, Clone)]
pub struct ContextCompilerInput<'a> {
    pub lane: EngineExecutionLane,
    pub user_prompt: &'a str,
    pub response_depth_mode: &'a str,
    pub stage_route: Option<&'a StageRoute>,
    pub recall_readiness: RecallReadiness,
}

#[derive(Debug, Clone)]
pub struct ContextCompilerOutput {
    pub compiled_prompt: String,
    pub lane_policy_profile: &'static str,
    pub allow_no_tools_fallback: bool,
    pub compiler_summary: String,
}

pub fn compile_context_prompt(input: ContextCompilerInput<'_>) -> ContextCompilerOutput {
    let lane_policy_profile = default_policy_profile_for_lane(input.lane);
    let allow_no_tools_fallback = input.recall_readiness == RecallReadiness::Verified;
    let fallback_gate = if allow_no_tools_fallback {
        "enabled"
    } else {
        "disabled"
    };

    let mut prompt = String::new();
    prompt.push_str(input.user_prompt.trim());
    prompt.push_str("\n\n[MEDOUSA_CONTEXT_COMPILER]\n");
    prompt.push_str("version=v1\n");
    prompt.push_str(&format!("lane={}\n", input.lane.as_str()));
    prompt.push_str(&format!("lane_policy_profile={}\n", lane_policy_profile));
    prompt.push_str(&format!(
        "recall_readiness={}\n",
        input.recall_readiness.as_str()
    ));
    prompt.push_str(&format!("no_tools_fallback={}\n", fallback_gate));

    prompt.push_str("\n[MEDOUSA_RESPONSE_DEPTH]\n");
    prompt.push_str(&format!("mode={}\n", input.response_depth_mode.trim()));
    prompt.push_str(
        "policy=Use concise mode for short output, standard for balanced output, deep for detailed evidence-forward explanation.\n",
    );

    if let Some(route) = input.stage_route {
        prompt.push_str("\n[MEDOUSA_STAGE_ROUTE]\n");
        prompt.push_str(&format!("role={}\n", route.role));
        prompt.push_str(&format!("provider={}\n", route.provider));
        prompt.push_str(&format!("model={}\n", route.model));
        prompt.push_str(&format!("policy_profile={}\n", route.policy_profile));
        prompt.push_str(&format!("fallback_chain={}\n", route.fallback_chain.join(",")));
    }

    let compiler_summary = format!(
        "context_compiler=v1 lane={} recall={} no_tools_fallback={} policy={}",
        input.lane.as_str(),
        input.recall_readiness.as_str(),
        fallback_gate,
        lane_policy_profile,
    );

    ContextCompilerOutput {
        compiled_prompt: prompt,
        lane_policy_profile,
        allow_no_tools_fallback,
        compiler_summary,
    }
}

pub fn compile_default_lane_prompt(lane: EngineExecutionLane, user_prompt: &str) -> String {
    compile_context_prompt(ContextCompilerInput {
        lane,
        user_prompt,
        response_depth_mode: "standard",
        stage_route: None,
        recall_readiness: RecallReadiness::Missing,
    })
    .compiled_prompt
}

#[cfg(test)]
mod tests {
    use super::{
        ContextCompilerInput, EngineExecutionLane, HeartbeatAction, HeartbeatSignals,
        LaneSafetyActionClass, RecallReadiness, compile_context_prompt,
        compile_default_lane_prompt, default_heartbeat_lane_policy,
        default_policy_profile_for_lane, evaluate_heartbeat_significance,
        lane_accepts_policy_profile, lane_execution_budget, validate_lane_action,
        validate_lane_policy_profile,
    };

    #[test]
    fn compiler_emits_lane_and_depth_sections() {
        let output = compile_context_prompt(ContextCompilerInput {
            lane: EngineExecutionLane::Interactive,
            user_prompt: "Summarize current status",
            response_depth_mode: "standard",
            stage_route: None,
            recall_readiness: RecallReadiness::Missing,
        });

        assert!(output.compiled_prompt.contains("[MEDOUSA_CONTEXT_COMPILER]"));
        assert!(output.compiled_prompt.contains("lane=interactive"));
        assert!(output.compiled_prompt.contains("[MEDOUSA_RESPONSE_DEPTH]"));
        assert!(!output.allow_no_tools_fallback);
    }

    #[test]
    fn no_tools_fallback_requires_verified_recall() {
        let verified = compile_context_prompt(ContextCompilerInput {
            lane: EngineExecutionLane::Interactive,
            user_prompt: "Explain architecture",
            response_depth_mode: "deep",
            stage_route: None,
            recall_readiness: RecallReadiness::Verified,
        });
        let unverified = compile_context_prompt(ContextCompilerInput {
            lane: EngineExecutionLane::Interactive,
            user_prompt: "Explain architecture",
            response_depth_mode: "deep",
            stage_route: None,
            recall_readiness: RecallReadiness::Unverified,
        });

        assert!(verified.allow_no_tools_fallback);
        assert!(!unverified.allow_no_tools_fallback);
    }

    #[test]
    fn lane_policy_and_budget_defaults_are_stable() {
        assert_eq!(
            default_policy_profile_for_lane(EngineExecutionLane::Scheduled),
            "scheduled"
        );

        let heartbeat = lane_execution_budget(EngineExecutionLane::Heartbeat);
        assert_eq!(heartbeat.max_classifier_calls, 0);
        assert_eq!(heartbeat.max_continuations, 0);
    }

    #[test]
    fn heartbeat_defaults_to_noop_when_signal_is_low() {
        let decision = evaluate_heartbeat_significance(
            &HeartbeatSignals::default(),
            default_heartbeat_lane_policy(),
            None,
        );

        assert_eq!(decision.action, HeartbeatAction::Noop);
        assert!(decision.significance < 0.65);
    }

    #[test]
    fn heartbeat_static_dead_letter_does_not_renotify() {
        let decision = evaluate_heartbeat_significance(
            &HeartbeatSignals {
                dead_letter_jobs: 5,
                ..HeartbeatSignals::default()
            },
            default_heartbeat_lane_policy(),
            Some(5),
        );

        assert_eq!(decision.action, HeartbeatAction::Noop);
        assert!(decision.reason.contains("dead_letter_static"));
    }

    #[test]
    fn heartbeat_notifies_on_dead_letter_increase() {
        let decision = evaluate_heartbeat_significance(
            &HeartbeatSignals {
                dead_letter_jobs: 6,
                ..HeartbeatSignals::default()
            },
            default_heartbeat_lane_policy(),
            Some(5),
        );

        assert_eq!(decision.action, HeartbeatAction::Notify);
        assert!(decision.reason.contains("dead_letter_detected"));
    }

    #[test]
    fn default_lane_prompt_compiler_emits_scheduled_lane_metadata() {
        let compiled = compile_default_lane_prompt(EngineExecutionLane::Scheduled, "Run report");
        assert!(compiled.contains("[MEDOUSA_CONTEXT_COMPILER]"));
        assert!(compiled.contains("lane=scheduled"));
        assert!(compiled.contains("lane_policy_profile=scheduled"));
        assert!(compiled.contains("recall_readiness=missing"));
    }

    #[test]
    fn lane_safety_matrix_allows_recurring_registration_on_interactive_lane() {
        let result = validate_lane_action(
            EngineExecutionLane::Interactive,
            LaneSafetyActionClass::RecurringRegistration,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn lane_safety_matrix_allows_heartbeat_dispatch_for_scheduled_lane() {
        let result = validate_lane_action(
            EngineExecutionLane::Scheduled,
            LaneSafetyActionClass::HeartbeatNotificationDispatch,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn lane_policy_profile_validation_rejects_mismatch() {
        assert!(!lane_accepts_policy_profile(
            EngineExecutionLane::Interactive,
            "scheduled"
        ));
        assert!(validate_lane_policy_profile(
            EngineExecutionLane::Interactive,
            Some("scheduled")
        )
        .is_err());
    }

    #[test]
    fn lane_policy_profile_validation_accepts_matching_profile() {
        assert!(validate_lane_policy_profile(
            EngineExecutionLane::Scheduled,
            Some("scheduled")
        )
        .is_ok());
    }
}