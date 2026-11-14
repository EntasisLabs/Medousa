use super::*;

fn world() -> World {
    let mut w = World::new();
    w.insert_resource(Approvals::default());
    w
}
fn failed(session: &str) -> ApprovalUpdate {
    ApprovalUpdate {
        command: refresh(session),
        result: Err("offline".into()),
    }
}
#[test]
fn errors_and_updates_are_scoped_to_originating_session() {
    let mut w = world();
    apply(&mut w, failed("old"));
    let s = w.resource::<Approvals>().unwrap();
    assert_eq!(s.sessions["old"].errors["refresh"], "offline");
    assert!(!s.sessions.contains_key("new"));
}
#[test]
fn failed_action_releases_pending_and_retains_error() {
    let mut w = world();
    let c = ApprovalCommand {
        session: "s".into(),
        serial: serial(),
        operation: Operation::Budget {
            id: "exact-id".into(),
            rounds: Some(9),
            approve: true,
        },
    };
    let s = w
        .resource_mut::<Approvals>()
        .unwrap()
        .sessions
        .entry("s".into())
        .or_default();
    s.pending.insert(c.operation.key());
    apply(
        &mut w,
        ApprovalUpdate {
            command: c,
            result: Err("permission denied".into()),
        },
    );
    let s = &w.resource::<Approvals>().unwrap().sessions["s"];
    assert!(s.pending.is_empty());
    assert_eq!(s.errors["budget:exact-id"], "permission denied");
}
#[test]
fn stale_refresh_cannot_clear_later_action_error() {
    let mut w = world();
    let old = refresh("s");
    let new = refresh("s");
    let latest = new.serial;
    apply(
        &mut w,
        ApprovalUpdate {
            command: new,
            result: Err("latest error".into()),
        },
    );
    apply(
        &mut w,
        ApprovalUpdate {
            command: old,
            result: Ok(Payload::Inbox {
                budgets: Ok(vec![]),
                permissions: Ok(vec![]),
                proposals: Ok(vec![]),
            }),
        },
    );
    let s = &w.resource::<Approvals>().unwrap().sessions["s"];
    assert_eq!(s.latest["refresh"], latest);
    assert_eq!(s.errors["refresh"], "latest error");
}
#[test]
fn operation_identity_preserves_channel_and_proposal() {
    let approve = Operation::Proposal {
        channel: "channel/a".into(),
        id: "opaque/id".into(),
        verb: ProposalVerb::Approve,
    };
    let start = Operation::Proposal {
        channel: "channel/a".into(),
        id: "opaque/id".into(),
        verb: ProposalVerb::Start,
    };
    assert_eq!(approve.key(), start.key());
    assert!(matches!(
        approve,
        Operation::Proposal {
            verb: ProposalVerb::Approve,
            ..
        }
    ));
    assert!(matches!(
        start,
        Operation::Proposal {
            verb: ProposalVerb::Start,
            ..
        }
    ));
}
fn permission(session: &str) -> AgentPermissionRequestRecord {
    serde_json::from_value(serde_json::json!({
        "request_id":"request/opaque","agent_session_id":"agent","session_id":session,"runtime":"acp","summary":"Choose scope",
        "options":[{"option_id":"allow:always/opaque","name":"Always allow","kind":"allow_always"},{"option_id":"reject:opaque","name":"Reject","kind":"reject_once"}],
        "status":"pending","created_at_utc":"2026-11-14T00:00:00Z","updated_at_utc":"2026-11-14T00:00:00Z"
    })).unwrap()
}
fn fixture() -> (World, Entity) {
    let mut w = world();
    let parent = w.spawn((Layout::column(),));
    let root = setup(&mut w, parent);
    (w, root)
}
fn activate(w: &mut World, e: Entity) -> bool {
    let name = w.get::<Button>(e).unwrap().action.clone();
    action(w, &ActionIntent { entity: e, name })
}
#[test]
fn exact_acp_option_is_queued_once_without_resolving_optimistically() {
    let (mut w, root) = fixture();
    let s = w
        .resource_mut::<Approvals>()
        .unwrap()
        .sessions
        .entry("s".into())
        .or_default();
    s.permissions.push(permission("s"));
    s.revision += 1;
    sync(&mut w, root, Some("s"), None);
    let state = w.resource::<Approvals>().unwrap();
    let entity=*state.actions.iter().find(|(_,c)| matches!(&c.operation,Operation::Permission {option:Some(o),..} if o=="reject:opaque")).unwrap().0;
    assert!(activate(&mut w, entity));
    assert!(activate(&mut w, entity));
    let commands = drain_commands(&mut w);
    assert_eq!(commands.len(), 1);
    assert!(
        matches!(&commands[0].operation,Operation::Permission {id,option:Some(o),approve:true} if id=="request/opaque" && o=="reject:opaque")
    );
    assert_eq!(
        w.resource::<Approvals>().unwrap().sessions["s"]
            .permissions
            .len(),
        1
    );
}
#[test]
fn session_switch_invalidates_old_controls_and_keeps_old_error() {
    let (mut w, root) = fixture();
    apply(&mut w, failed("old"));
    sync(&mut w, root, Some("old"), None);
    let old = *w
        .resource::<Approvals>()
        .unwrap()
        .actions
        .keys()
        .next()
        .unwrap();
    sync(&mut w, root, Some("new"), None);
    assert!(!action(
        &mut w,
        &ActionIntent {
            entity: old,
            name: "anything".into()
        }
    ));
    assert!(drain_commands(&mut w).is_empty());
    assert_eq!(
        w.resource::<Approvals>().unwrap().sessions["old"].errors["refresh"],
        "offline"
    );
}
#[test]
fn stale_inbox_cannot_resurrect_resolved_permission() {
    let mut w = world();
    let old = refresh("s");
    let command = ApprovalCommand {
        session: "s".into(),
        serial: serial(),
        operation: Operation::Permission {
            id: "request/opaque".into(),
            option: None,
            approve: false,
        },
    };
    let mut row = permission("s");
    row.status = AgentPermissionRequestStatus::Denied;
    apply(
        &mut w,
        ApprovalUpdate {
            command,
            result: Ok(Payload::Permission(row)),
        },
    );
    apply(
        &mut w,
        ApprovalUpdate {
            command: old,
            result: Ok(Payload::Inbox {
                budgets: Ok(vec![]),
                permissions: Ok(vec![permission("s")]),
                proposals: Ok(vec![]),
            }),
        },
    );
    assert!(
        w.resource::<Approvals>().unwrap().sessions["s"]
            .permissions
            .is_empty()
    );
}
#[test]
fn partial_refresh_preserves_errors_and_available_rows() {
    let mut w = world();
    apply(
        &mut w,
        ApprovalUpdate {
            command: refresh("s"),
            result: Ok(Payload::Inbox {
                budgets: Err("budget offline".into()),
                permissions: Ok(vec![permission("s")]),
                proposals: Err("proposal offline".into()),
            }),
        },
    );
    let s = &w.resource::<Approvals>().unwrap().sessions["s"];
    assert_eq!(s.permissions.len(), 1);
    assert_eq!(s.errors.len(), 2);
}
fn budget(rounds: usize) -> TurnBudgetRequestRecord {
    serde_json::from_value(serde_json::json!({
        "request_id":"budget-exact","stream_turn_id":1,"session_id":"s","rounds_executed":10,"max_tool_rounds":10,"requested_rounds":rounds,
        "reason":"Finish verification","status":"pending","created_at_utc":"2026-11-14T00:00:00Z","updated_at_utc":"2026-11-14T00:00:00Z"
    })).unwrap()
}
#[test]
fn budget_allowance_uses_requested_default_and_omits_zero() {
    for (requested, expected) in [(12, Some(12)), (0, None)] {
        let (mut w, root) = fixture();
        let s = w
            .resource_mut::<Approvals>()
            .unwrap()
            .sessions
            .entry("s".into())
            .or_default();
        s.budgets.push(budget(requested));
        s.revision += 1;
        sync(&mut w, root, Some("s"), None);
        let entity = *w
            .resource::<Approvals>()
            .unwrap()
            .actions
            .iter()
            .find(|(_, c)| matches!(c.operation, Operation::Budget { approve: true, .. }))
            .unwrap()
            .0;
        assert!(activate(&mut w, entity));
        let c = drain_commands(&mut w).pop().unwrap();
        assert!(
            matches!(c.operation,Operation::Budget {rounds,id,approve:true} if rounds==expected && id=="budget-exact")
        );
    }
}
