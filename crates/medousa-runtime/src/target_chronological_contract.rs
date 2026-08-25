//! Executable target contract for chronological turns.
//!
//! The production golden tests in `golden_turn` characterize the currently
//! shipped PackHold loop. These fixtures describe the locked V3 destination
//! without changing production behavior during Phase 0. Later runtime, stream,
//! persistence, and client tests should consume the same fixture cases.

use serde::Deserialize;

const CONTRACT_FIXTURE: &str = include_str!("../testdata/chronological_turn_contract_v3.json");

#[derive(Debug, Deserialize)]
struct ContractFixture {
    schema_version: u32,
    cases: Vec<ContractCase>,
}

#[derive(Debug, Deserialize)]
struct ContractCase {
    id: String,
    rounds: Vec<ModelRound>,
    #[serde(default)]
    runtime_terminal: Option<RuntimeTerminal>,
    expected: Vec<ContractEvent>,
}

#[derive(Debug, Deserialize)]
struct ModelRound {
    model_round: usize,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    actions: Vec<RoundAction>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RoundAction {
    ToolGroup {
        tools: Vec<ToolFixture>,
    },
    Finish {
        #[serde(default)]
        message: Option<String>,
    },
    RequestInput {
        #[serde(default)]
        message: Option<String>,
    },
    Checkpoint {
        #[serde(default)]
        message: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolFixture {
    run_id: String,
    name: String,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeTerminal {
    outcome: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ContractEvent {
    AssistantTextStarted {
        segment_id: String,
        model_round: usize,
    },
    ContentAppend {
        segment_id: String,
        text: String,
    },
    AssistantTextCommitted {
        segment_id: String,
    },
    ToolStarted {
        run_id: String,
        name: String,
        tool_round: usize,
    },
    ToolFinished {
        run_id: String,
        status: String,
    },
    TurnCompleted {
        outcome: String,
        aggregate_text: String,
    },
}

#[derive(Debug, Default)]
struct TargetProjector {
    active_work: bool,
    terminal: bool,
    text_ordinal: usize,
    visible_text: Vec<String>,
    events: Vec<ContractEvent>,
}

impl TargetProjector {
    fn project_case(mut self, case: &ContractCase) -> Result<Vec<ContractEvent>, String> {
        let mut prior_round = 0;
        for round in &case.rounds {
            if self.terminal {
                return Err(format!(
                    "case {} has model round {} after terminal",
                    case.id, round.model_round
                ));
            }
            if round.model_round <= prior_round {
                return Err(format!(
                    "case {} model rounds must increase strictly",
                    case.id
                ));
            }
            prior_round = round.model_round;

            let round_has_text = round
                .text
                .as_deref()
                .map(str::trim)
                .is_some_and(|text| !text.is_empty());
            if let Some(text) = round
                .text
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                self.commit_text(&case.id, round.model_round, text);
            }

            if round.actions.is_empty() {
                if round_has_text && !self.active_work {
                    self.complete("completed");
                }
                continue;
            }

            let tool_groups = round
                .actions
                .iter()
                .filter(|action| matches!(action, RoundAction::ToolGroup { .. }))
                .count();
            let terminal_actions = round.actions.len().saturating_sub(tool_groups);
            if terminal_actions > 1 || (terminal_actions == 1 && tool_groups > 0) {
                return Err(format!(
                    "case {} mixes terminal and ordinary actions in round {}",
                    case.id, round.model_round
                ));
            }

            for action in &round.actions {
                if self.terminal {
                    return Err(format!(
                        "case {} has an action after a terminal action in round {}",
                        case.id, round.model_round
                    ));
                }
                match action {
                    RoundAction::ToolGroup { tools } => {
                        if tools.is_empty() {
                            return Err(format!("case {} contains an empty tool group", case.id));
                        }
                        self.active_work = true;
                        for tool in tools {
                            self.events.push(ContractEvent::ToolStarted {
                                run_id: tool.run_id.clone(),
                                name: tool.name.clone(),
                                tool_round: round.model_round,
                            });
                        }
                        for tool in tools {
                            self.events.push(ContractEvent::ToolFinished {
                                run_id: tool.run_id.clone(),
                                status: tool.status.clone(),
                            });
                        }
                    }
                    RoundAction::Finish { message } => {
                        self.commit_control_message_if_needed(
                            &case.id,
                            round.model_round,
                            round_has_text,
                            message.as_deref(),
                        );
                        self.complete("completed");
                    }
                    RoundAction::RequestInput { message } => {
                        self.commit_control_message_if_needed(
                            &case.id,
                            round.model_round,
                            round_has_text,
                            message.as_deref(),
                        );
                        self.complete("needs_input");
                    }
                    RoundAction::Checkpoint { message } => {
                        self.commit_control_message_if_needed(
                            &case.id,
                            round.model_round,
                            round_has_text,
                            message.as_deref(),
                        );
                        self.complete("checkpointed");
                    }
                }
            }
        }

        if !self.terminal {
            let terminal = case.runtime_terminal.as_ref().ok_or_else(|| {
                format!(
                    "case {} leaves an active turn without a runtime terminal",
                    case.id
                )
            })?;
            self.complete(&terminal.outcome);
        } else if case.runtime_terminal.is_some() {
            return Err(format!(
                "case {} declares both an action terminal and runtime terminal",
                case.id
            ));
        }

        Ok(self.events)
    }

    fn commit_control_message_if_needed(
        &mut self,
        case_id: &str,
        model_round: usize,
        round_has_text: bool,
        message: Option<&str>,
    ) {
        if round_has_text {
            return;
        }
        if let Some(message) = message.map(str::trim).filter(|message| !message.is_empty()) {
            self.commit_text(case_id, model_round, message);
        }
    }

    fn commit_text(&mut self, case_id: &str, model_round: usize, text: &str) {
        self.text_ordinal += 1;
        let segment_id = format!("{case_id}:text:{}", self.text_ordinal);
        self.events.push(ContractEvent::AssistantTextStarted {
            segment_id: segment_id.clone(),
            model_round,
        });
        self.events.push(ContractEvent::ContentAppend {
            segment_id: segment_id.clone(),
            text: text.to_string(),
        });
        self.events
            .push(ContractEvent::AssistantTextCommitted { segment_id });
        self.visible_text.push(text.to_string());
    }

    fn complete(&mut self, outcome: &str) {
        self.events.push(ContractEvent::TurnCompleted {
            outcome: outcome.to_string(),
            aggregate_text: self.visible_text.join("\n\n"),
        });
        self.terminal = true;
    }
}

#[test]
fn chronological_v3_target_fixtures_are_executable_and_self_consistent() {
    let fixture: ContractFixture =
        serde_json::from_str(CONTRACT_FIXTURE).expect("parse chronological V3 fixture");
    assert_eq!(fixture.schema_version, 3);
    assert!(!fixture.cases.is_empty());

    for case in &fixture.cases {
        let actual = TargetProjector::default()
            .project_case(case)
            .unwrap_or_else(|error| panic!("invalid target fixture: {error}"));
        assert_eq!(actual, case.expected, "target case {} drifted", case.id);
        assert!(matches!(
            actual.last(),
            Some(ContractEvent::TurnCompleted { .. })
        ));
    }
}

#[test]
fn target_fixture_covers_the_locked_semantic_boundaries() {
    let fixture: ContractFixture =
        serde_json::from_str(CONTRACT_FIXTURE).expect("parse chronological V3 fixture");
    let ids = fixture
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<Vec<_>>();

    for required in [
        "direct_response",
        "chronological_work_with_naked_prose",
        "finish_message_fallback",
        "checkpoint_after_visible_prose",
        "request_input_after_visible_prose",
        "failure_preserves_partial_timeline",
    ] {
        assert!(ids.contains(&required), "missing target case {required}");
    }
}

#[test]
fn target_contract_rejects_terminal_and_ordinary_actions_in_one_round() {
    let case = ContractCase {
        id: "invalid_mixed_terminal".to_string(),
        rounds: vec![ModelRound {
            model_round: 1,
            text: Some("This cannot finish before its receipt exists.".to_string()),
            actions: vec![
                RoundAction::ToolGroup {
                    tools: vec![ToolFixture {
                        run_id: "probe".to_string(),
                        name: "data_probe".to_string(),
                        status: "succeeded".to_string(),
                    }],
                },
                RoundAction::Finish { message: None },
            ],
        }],
        runtime_terminal: None,
        expected: Vec::new(),
    };

    let error = TargetProjector::default()
        .project_case(&case)
        .expect_err("mixed terminal action must be rejected");
    assert!(error.contains("mixes terminal and ordinary actions"));
}

#[test]
fn raw_v3_facts_support_consumer_defined_projections() {
    let fixture: ContractFixture =
        serde_json::from_str(CONTRACT_FIXTURE).expect("parse chronological V3 fixture");
    let case = fixture
        .cases
        .iter()
        .find(|case| case.id == "chronological_work_with_naked_prose")
        .expect("projection fixture case");
    let events = TargetProjector::default()
        .project_case(case)
        .expect("project target case");

    let tool_runs = events
        .iter()
        .filter_map(|event| match event {
            ContractEvent::ToolStarted { run_id, .. }
            | ContractEvent::ToolFinished { run_id, .. } => Some(run_id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        tool_runs,
        [
            "probe-a", "probe-b", "probe-a", "probe-b", "probe-c", "probe-c"
        ]
    );

    let selected_segment = "chronological_work_with_naked_prose:text:3";
    let selected_text = events
        .iter()
        .filter_map(|event| match event {
            ContractEvent::ContentAppend { segment_id, text } if segment_id == selected_segment => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect::<String>();
    assert_eq!(
        selected_text,
        "I’m checking that gap against the durable record."
    );
}
