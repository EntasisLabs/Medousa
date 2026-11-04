//! ApplyProposals stores thin facts so snapshot.agent_groups include proposal anchors.

use medousa_app_core::contract::{
    ChatCommand, ChatUpdate, ProposalFact, ProposalSourceFact,
};
use medousa_app_core::ChatCore;

#[test]
fn apply_proposals_lands_on_snapshot_agent_groups() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses".into(),
        authority_id: "auth".into(),
    })
    .unwrap();

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::ApplyProposals {
            session_id: "ses".into(),
            proposals: vec![ProposalFact {
                proposal_id: "p1".into(),
                assignment_id: Some("a1".into()),
                created_at: Some("2026-11-04T00:00:00Z".into()),
                execution_runtime_id: Some("local".into()),
                binding_authority_id: None,
                binding_session_id: None,
                sources: vec![ProposalSourceFact {
                    authority_id: "auth".into(),
                    session_id: "ses".into(),
                    after_entry_seq: None,
                    through_entry_seq: 1,
                }],
            }],
            visible_item_ids: None,
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };

    assert!(
        snapshot
            .agent_groups
            .iter()
            .any(|group| group.entries.iter().any(|entry| {
                matches!(
                    entry,
                    medousa_app_core::contract::AgentPlacementEntry::Proposal { proposal_id, .. }
                        if proposal_id == "p1"
                )
            })),
        "snapshot agent_groups should include the applied proposal"
    );
}

#[test]
fn apply_proposals_clears_when_empty() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses".into(),
        authority_id: "auth".into(),
    })
    .unwrap();
    core.dispatch(ChatCommand::ApplyProposals {
        session_id: "ses".into(),
        proposals: vec![ProposalFact {
            proposal_id: "p1".into(),
            assignment_id: None,
            created_at: None,
            execution_runtime_id: None,
            binding_authority_id: None,
            binding_session_id: None,
            sources: vec![],
        }],
        visible_item_ids: None,
        expected_revision: None,
    })
    .unwrap();

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::ApplyProposals {
            session_id: "ses".into(),
            proposals: vec![],
            visible_item_ids: None,
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert!(
        snapshot.agent_groups.iter().all(|group| {
            group.entries.iter().all(|entry| {
                !matches!(
                    entry,
                    medousa_app_core::contract::AgentPlacementEntry::Proposal { .. }
                )
            })
        }),
        "empty ApplyProposals should drop proposal anchors from agent_groups"
    );
}
