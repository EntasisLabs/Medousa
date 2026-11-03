//! Port of apps/medousa-home chatAgentGroups.test.ts — placement owns anchors,
//! proposal threading, mixed spawn order, and never-treat-worker-lane-as-spawn.

use medousa_app_core::contract::{
    AgentPlacementEntry, ChatItem, ChatItemRole, ChatLane, ChatSegment, ProposalFact,
    ProposalSourceFact, ToolRunState, TranscriptCoordinate, TranscriptSourceRef,
    WorkerPlacementFact,
};
use medousa_app_core::project_agent_placement;

fn user(id: &str, seq: u64) -> ChatItem {
    ChatItem {
        id: id.into(),
        role: ChatItemRole::User,
        content: id.into(),
        transcript: Some(TranscriptCoordinate {
            authority_id: "local".into(),
            session_id: "chat".into(),
            entry_id: id.into(),
            entry_seq: seq,
            source: None,
        }),
        ..empty()
    }
}

fn assistant(id: &str, seq: u64) -> ChatItem {
    ChatItem {
        id: id.into(),
        role: ChatItemRole::Assistant,
        content: id.into(),
        transcript: Some(TranscriptCoordinate {
            authority_id: "local".into(),
            session_id: "chat".into(),
            entry_id: id.into(),
            entry_seq: seq,
            source: None,
        }),
        ..empty()
    }
}

fn empty() -> ChatItem {
    ChatItem {
        id: String::new(),
        role: ChatItemRole::Assistant,
        content: String::new(),
        created_at: None,
        streaming: None,
        turn_id: None,
        turn_index: None,
        lane: None,
        ask_job_id: None,
        work_id: None,
        phase: None,
        status_line: None,
        tools: None,
        tool_runs: None,
        segments: None,
        ui_artifacts: None,
        reasoning: None,
        answer_state: None,
        response_provider: None,
        response_model: None,
        stage_whisper: None,
        budget_request_id: None,
        requested_rounds: None,
        failed: None,
        error_line: None,
        error_detail: None,
        speaker_profile_id: None,
        media_attachments: None,
        host_context: None,
        reactions: None,
        live_transcripts: None,
        transcript: None,
    }
}

fn base_messages() -> Vec<ChatItem> {
    vec![
        user("request", 1),
        assistant("spawn", 2),
        user("later-request", 3),
        assistant("later-reply", 4),
    ]
}

fn proposal(id: &str, through: u64, authority: &str) -> ProposalFact {
    ProposalFact {
        proposal_id: id.into(),
        assignment_id: None,
        created_at: None,
        execution_runtime_id: None,
        binding_authority_id: None,
        binding_session_id: None,
        sources: vec![ProposalSourceFact {
            authority_id: authority.into(),
            session_id: "chat".into(),
            after_entry_seq: None,
            through_entry_seq: through,
        }],
    }
}

fn visible_ids(items: &[ChatItem]) -> Vec<String> {
    items.iter().map(|item| item.id.clone()).collect()
}

fn anchors(groups: &[medousa_app_core::AgentPlacementGroup]) -> Vec<Option<String>> {
    groups
        .iter()
        .map(|group| group.anchor_item_id.clone())
        .collect()
}

fn entry_ids(group: &medousa_app_core::AgentPlacementGroup) -> Vec<String> {
    group
        .entries
        .iter()
        .map(|entry| match entry {
            AgentPlacementEntry::Worker { work_id, .. } => work_id.clone(),
            AgentPlacementEntry::Proposal { proposal_id, .. } => proposal_id.clone(),
        })
        .collect()
}

fn entry_kinds(group: &medousa_app_core::AgentPlacementGroup) -> Vec<&'static str> {
    group
        .entries
        .iter()
        .map(|entry| match entry {
            AgentPlacementEntry::Worker { .. } => "worker",
            AgentPlacementEntry::Proposal { .. } => "proposal",
        })
        .collect()
}

fn entry_nodes(group: &medousa_app_core::AgentPlacementGroup) -> Vec<Option<String>> {
    group
        .entries
        .iter()
        .map(|entry| match entry {
            AgentPlacementEntry::Worker { node_id, .. }
            | AgentPlacementEntry::Proposal { node_id, .. } => node_id.clone(),
        })
        .collect()
}

#[test]
fn keeps_one_durable_conversation_at_original_spawn() {
    let messages = base_messages();
    let mut first = proposal("first", 1, "local");
    let mut followup = proposal("followup", 3, "local");
    first.created_at = Some("2026-10-30T14:00:00Z".into());
    followup.created_at = Some("2026-10-30T14:01:00Z".into());
    first.execution_runtime_id = Some("runtime".into());
    followup.execution_runtime_id = Some("runtime".into());
    first.binding_authority_id = Some("local".into());
    first.binding_session_id = Some("durable".into());
    followup.binding_authority_id = Some("local".into());
    followup.binding_session_id = Some("durable".into());

    let groups = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[followup.clone(), first.clone()],
        &[],
        None,
        None,
    );
    assert_eq!(anchors(&groups), vec![Some("request".into())]);
    assert_eq!(entry_ids(&groups[0]), vec!["followup".to_string()]);

    let mut foreign = followup.clone();
    foreign.proposal_id = "foreign".into();
    foreign.binding_authority_id = Some("another-authority".into());
    let groups = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[first, foreign],
        &[],
        None,
        None,
    );
    let proposals: Vec<_> = groups
        .iter()
        .flat_map(|group| group.entries.iter())
        .filter(|entry| matches!(entry, AgentPlacementEntry::Proposal { .. }))
        .collect();
    assert_eq!(proposals.len(), 2);
}

#[test]
fn orders_mixed_peers_and_workers_by_spawn_segments() {
    let mut messages = base_messages();
    messages[1].segments = Some(vec![
        ChatSegment::ToolGroup {
            group_id: "first".into(),
            tool_round: 1,
            runs: vec![ToolRunState {
                run_id: "one".into(),
                tool_name: "workshop.spawn".into(),
                status: "succeeded".into(),
                round: 1,
                input_summary: None,
                input_params: None,
                output_summary: Some("work-z".into()),
                artifact_refs: None,
            }],
        },
        ChatSegment::ToolGroup {
            group_id: "second".into(),
            tool_round: 2,
            runs: vec![ToolRunState {
                run_id: "two".into(),
                tool_name: "peer_handoff".into(),
                status: "succeeded".into(),
                round: 2,
                input_summary: None,
                input_params: None,
                output_summary: Some("peer-middle".into()),
                artifact_refs: None,
            }],
        },
        ChatSegment::Handoff {
            handoff_kind: "worker".into(),
            text: "Delegated".into(),
            work_id: Some("work-a".into()),
        },
    ]);
    // Insertion order matches the TS Map built from ['work-a','work-z']; segment
    // coordinates still decide mixed spawn order.
    let workers = vec![
        WorkerPlacementFact {
            work_id: "work-a".into(),
            parent_turn_id: None,
            parent_item_id: Some("spawn".into()),
        },
        WorkerPlacementFact {
            work_id: "work-z".into(),
            parent_turn_id: None,
            parent_item_id: Some("spawn".into()),
        },
    ];
    let group = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[proposal("peer-middle", 1, "local")],
        &workers,
        None,
        None,
    )
    .into_iter()
    .find(|group| group.anchor_item_id.as_deref() == Some("request"))
    .expect("request group");
    assert_eq!(
        entry_ids(&group),
        vec![
            "work-z".to_string(),
            "peer-middle".to_string(),
            "work-a".to_string()
        ]
    );
    assert_eq!(
        entry_nodes(&group),
        vec![
            Some("spawn:segment:first".into()),
            Some("spawn:segment:second".into()),
            Some("spawn:segment:handoff:work-a:2".into()),
        ]
    );
}

#[test]
fn orders_mixed_delegations_inside_same_tool_group_by_call_order() {
    let mut messages = base_messages();
    messages[1].segments = Some(vec![ChatSegment::ToolGroup {
        group_id: "batch".into(),
        tool_round: 1,
        runs: vec![
            ToolRunState {
                run_id: "first".into(),
                tool_name: "workshop.spawn".into(),
                status: "succeeded".into(),
                round: 1,
                input_summary: None,
                input_params: None,
                output_summary: Some("work-z".into()),
                artifact_refs: None,
            },
            ToolRunState {
                run_id: "second".into(),
                tool_name: "peer_handoff".into(),
                status: "succeeded".into(),
                round: 2,
                input_summary: None,
                input_params: None,
                output_summary: Some("peer".into()),
                artifact_refs: None,
            },
        ],
    }]);
    let group = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[proposal("peer", 1, "local")],
        &[WorkerPlacementFact {
            work_id: "work-z".into(),
            parent_turn_id: None,
            parent_item_id: Some("spawn".into()),
        }],
        None,
        None,
    )
    .into_iter()
    .find(|group| group.anchor_item_id.as_deref() == Some("request"))
    .unwrap();
    assert_eq!(entry_kinds(&group), vec!["worker", "proposal"]);
    assert_eq!(
        entry_nodes(&group),
        vec![
            Some("spawn:segment:batch".into()),
            Some("spawn:segment:batch".into())
        ]
    );
}

#[test]
fn preserves_admitted_worker_insertion_order_without_segments() {
    let messages = base_messages();
    let workers = vec![
        WorkerPlacementFact {
            work_id: "work-z".into(),
            parent_turn_id: None,
            parent_item_id: Some("spawn".into()),
        },
        WorkerPlacementFact {
            work_id: "work-a".into(),
            parent_turn_id: None,
            parent_item_id: Some("spawn".into()),
        },
    ];
    let group = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[],
        &workers,
        None,
        None,
    )
    .into_iter()
    .find(|group| group.anchor_item_id.as_deref() == Some("request"))
    .unwrap();
    assert_eq!(
        entry_ids(&group),
        vec!["work-z".to_string(), "work-a".to_string()]
    );
}

#[test]
fn never_matches_spawn_id_from_later_user_turn() {
    let mut messages = base_messages();
    messages[3].segments = Some(vec![ChatSegment::Handoff {
        handoff_kind: "worker".into(),
        text: "Delegated".into(),
        work_id: Some("work".into()),
    }]);
    let group = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[],
        &[WorkerPlacementFact {
            work_id: "work".into(),
            parent_turn_id: None,
            parent_item_id: Some("spawn".into()),
        }],
        None,
        None,
    )
    .into_iter()
    .find(|group| group.anchor_item_id.as_deref() == Some("request"))
    .unwrap();
    assert_eq!(entry_nodes(&group), vec![None]);
}

#[test]
fn uses_parent_turn_when_link_points_at_later_worker_result() {
    let mut messages = base_messages();
    messages[1].turn_id = Some("parent".into());
    messages.push(ChatItem {
        id: "result".into(),
        role: ChatItemRole::Assistant,
        content: "Done".into(),
        lane: Some(ChatLane::Worker),
        turn_id: Some("parent".into()),
        work_id: Some("work".into()),
        ..empty()
    });
    let groups = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[],
        &[WorkerPlacementFact {
            work_id: "work".into(),
            parent_turn_id: Some("parent".into()),
            parent_item_id: Some("result".into()),
        }],
        None,
        None,
    );
    assert_eq!(anchors(&groups), vec![Some("request".into())]);
}

#[test]
fn keeps_unknown_spawn_history_in_earlier_work() {
    let messages = vec![
        user("later-request", 3),
        assistant("later-reply", 4),
        ChatItem {
            id: "result".into(),
            role: ChatItemRole::Assistant,
            content: "Done".into(),
            lane: Some(ChatLane::Worker),
            work_id: Some("work".into()),
            ..empty()
        },
    ];
    for worker in [
        WorkerPlacementFact {
            work_id: "work".into(),
            parent_turn_id: None,
            parent_item_id: None,
        },
        WorkerPlacementFact {
            work_id: "work".into(),
            parent_turn_id: Some("missing".into()),
            parent_item_id: Some("missing".into()),
        },
    ] {
        let groups = project_agent_placement(
            &messages,
            &visible_ids(&messages),
            &[],
            &[worker],
            None,
            None,
        );
        assert_eq!(anchors(&groups), vec![None]);
    }
}

#[test]
fn groups_nine_agents_at_initiating_reply() {
    let messages = base_messages();
    let proposals: Vec<_> = (0..9)
        .map(|i| proposal(&format!("agent-{i}"), 1, "local"))
        .collect();
    let groups = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &proposals,
        &[],
        None,
        None,
    );
    assert_eq!(anchors(&groups), vec![Some("request".into())]);
    assert_eq!(groups[0].entries.len(), 9);
}

#[test]
fn keeps_separate_turns_in_separate_groups() {
    let messages = base_messages();
    let groups = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[proposal("first", 1, "local"), proposal("second", 3, "local")],
        &[],
        None,
        None,
    );
    let request = groups
        .iter()
        .find(|group| group.anchor_item_id.as_deref() == Some("request"))
        .unwrap();
    let later = groups
        .iter()
        .find(|group| group.anchor_item_id.as_deref() == Some("later-request"))
        .unwrap();
    assert_eq!(entry_ids(request), vec!["first".to_string()]);
    assert_eq!(entry_ids(later), vec!["second".to_string()]);
}

#[test]
fn does_not_attach_foreign_or_missing_history_to_latest_turn() {
    let messages = base_messages();
    let groups = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[proposal("foreign", 1, "other")],
        &[],
        None,
        None,
    );
    assert_eq!(anchors(&groups), vec![None]);
    assert_eq!(groups[0].entries.len(), 1);

    let later = messages[2..].to_vec();
    let groups = project_agent_placement(
        &later,
        &visible_ids(&later),
        &[proposal("older", 0, "local")],
        &[],
        None,
        None,
    );
    assert_eq!(anchors(&groups), vec![None]);
}

#[test]
fn uses_immutable_source_coordinates_for_imported_conversations() {
    let messages: Vec<_> = base_messages()
        .into_iter()
        .map(|mut item| {
            if let Some(transcript) = item.transcript.take() {
                item.transcript = Some(TranscriptCoordinate {
                    authority_id: "derived".into(),
                    session_id: "derived-chat".into(),
                    entry_id: transcript.entry_id.clone(),
                    entry_seq: transcript.entry_seq,
                    source: Some(TranscriptSourceRef {
                        authority_id: transcript.authority_id,
                        session_id: transcript.session_id,
                        entry_id: transcript.entry_id,
                        entry_seq: transcript.entry_seq,
                    }),
                });
            }
            item
        })
        .collect();
    let groups = project_agent_placement(
        &messages,
        &visible_ids(&messages),
        &[proposal("remote", 1, "local")],
        &[],
        None,
        None,
    );
    let request = groups
        .iter()
        .find(|group| group.anchor_item_id.as_deref() == Some("request"))
        .unwrap();
    assert_eq!(request.entries.len(), 1);
}

#[test]
fn anchors_newly_delegated_work_during_streaming_before_coordinates() {
    let mut live = base_messages()[..2].to_vec();
    live.push(ChatItem {
        id: "live-user".into(),
        role: ChatItemRole::User,
        content: "Delegate again".into(),
        created_at: Some("2026-10-09T12:00:00Z".into()),
        ..empty()
    });
    live.push(ChatItem {
        id: "live-reply".into(),
        role: ChatItemRole::Assistant,
        content: String::new(),
        streaming: Some(true),
        ..empty()
    });
    let mut row = proposal("new-work", 3, "local");
    row.created_at = Some("2026-10-09T12:00:01Z".into());
    let groups = project_agent_placement(
        &live,
        &visible_ids(&live),
        &[row.clone()],
        &[],
        Some("local"),
        Some("chat"),
    );
    assert!(
        groups
            .iter()
            .any(|group| group.anchor_item_id.as_deref() == Some("live-user")
                && group.entries.len() == 1)
    );

    let mut older = proposal("old-work", 1, "local");
    older.created_at = row.created_at.clone();
    let groups = project_agent_placement(
        &live,
        &visible_ids(&live),
        &[older],
        &[],
        Some("local"),
        Some("chat"),
    );
    assert!(
        groups
            .iter()
            .any(|group| group.anchor_item_id.as_deref() == Some("request")
                && group.entries.len() == 1)
    );
}

#[test]
fn keeps_worker_at_hidden_spawn_shell() {
    let mut messages = base_messages();
    let shell = ChatItem {
        id: "shell".into(),
        role: ChatItemRole::Assistant,
        content: String::new(),
        turn_id: Some("parent".into()),
        stage_whisper: Some("Delegated".into()),
        ..empty()
    };
    // Replace spawn with shell, keep later turns, add result.
    messages[1] = shell;
    messages.push(ChatItem {
        id: "result".into(),
        role: ChatItemRole::Assistant,
        content: "Result".into(),
        lane: Some(ChatLane::Worker),
        work_id: Some("work".into()),
        ..empty()
    });
    // Painted drops empty handoff shells.
    let painted: Vec<String> = messages
        .iter()
        .filter(|item| item.id != "shell")
        .map(|item| item.id.clone())
        .collect();
    let groups = project_agent_placement(
        &messages,
        &painted,
        &[],
        &[WorkerPlacementFact {
            work_id: "work".into(),
            parent_turn_id: Some("parent".into()),
            parent_item_id: Some("shell".into()),
        }],
        None,
        None,
    );
    assert!(
        groups
            .iter()
            .any(|group| group.anchor_item_id.as_deref() == Some("request")
                && group.entries.len() == 1)
    );
    assert!(!groups
        .iter()
        .any(|group| group.anchor_item_id.as_deref() == Some("later-reply")));
}
