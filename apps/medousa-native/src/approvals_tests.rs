use super::*;

fn proposal() -> PeerProposalReviewRecord {
    crate::ui::collaboration::peers::tests::row()
}
fn decision(row: &mut PeerProposalReviewRecord, approved: bool) {
    row.decision = Some(PeerProposalDecision {
        proposal_id: row.proposal.proposal_id.clone(),
        owner_principal_id: row.proposal.request.owner_principal_id.clone(),
        approved,
    });
}
fn completed(row: &mut PeerProposalReviewRecord, outcome: PeerAssignmentOutcome) {
    row.receipt = Some(ExternalPeerAssignmentReceipt {
        receipt_id: "receipt".into(),
        binding: row.binding.clone().unwrap(),
        outcome,
        result: "Recorded result remains in the exchange".into(),
    });
}
fn inbox(rows: Vec<PeerProposalReviewRecord>) -> ApprovalUpdate {
    ApprovalUpdate {
        command: refresh("parent"),
        result: Ok(Payload::Inbox {
            budgets: Ok(vec![]),
            permissions: Ok(vec![]),
            proposals: Ok(rows),
        }),
    }
}

#[test]
fn historical_proposals_never_mount_full_prompt_cards() {
    let (mut w, root) = fixture();
    let dispatched = proposal();
    let mut rows = vec![dispatched.clone()];
    for outcome in [
        PeerAssignmentOutcome::Completed,
        PeerAssignmentOutcome::Failed,
        PeerAssignmentOutcome::Cancelled,
        PeerAssignmentOutcome::Interrupted,
    ] {
        let mut row = dispatched.clone();
        completed(&mut row, outcome);
        rows.push(row.clone());
        row.binding = None; // A receipt alone also proves this is historical work.
        rows.push(row);
    }
    let mut declined = dispatched;
    declined.binding = None;
    decision(&mut declined, false);
    rows.push(declined);
    for row in rows {
        apply(&mut w, inbox(vec![row]));
        sync(&mut w, root, Some("parent"), None);
        assert!(children_of(&w, root).is_empty());
        assert!(w.resource::<Approvals>().unwrap().actions.is_empty());
        // Keep records for collaboration reading, not destructive inbox filtering.
        assert_eq!(
            w.resource::<Approvals>().unwrap().sessions["parent"]
                .proposals
                .len(),
            1
        );
    }
}

#[test]
fn proposal_controls_require_explicit_approve_then_start_and_disappear_on_dispatch() {
    let (mut w, root) = fixture();
    let mut row = proposal();
    row.binding = None;
    apply(&mut w, inbox(vec![row.clone()]));
    sync(&mut w, root, Some("parent"), None);
    let approve = *w
        .resource::<Approvals>()
        .unwrap()
        .actions
        .iter()
        .find(|(_, c)| {
            matches!(
                c.operation,
                Operation::Proposal {
                    verb: ProposalVerb::Approve,
                    ..
                }
            )
        })
        .unwrap()
        .0;
    assert!(activate(&mut w, approve));
    let command = drain_commands(&mut w).pop().unwrap();
    apply(
        &mut w,
        ApprovalUpdate {
            command,
            result: Ok(Payload::Proposal(PeerProposalActionResponse {
                proposal_id: row.proposal.proposal_id.clone(),
                binding: None,
            })),
        },
    );
    decision(&mut row, true);
    apply(&mut w, inbox(vec![row]));
    sync(&mut w, root, Some("parent"), None);
    let state = w.resource::<Approvals>().unwrap();
    assert!(state.actions.values().any(|c| matches!(
        c.operation,
        Operation::Proposal {
            verb: ProposalVerb::Start,
            ..
        }
    )));
    assert!(!state.actions.values().any(|c| matches!(
        c.operation,
        Operation::Proposal {
            verb: ProposalVerb::Approve,
            ..
        }
    )));
    apply(&mut w, inbox(vec![proposal()]));
    sync(&mut w, root, Some("parent"), None);
    assert!(children_of(&w, root).is_empty());
    assert!(w.resource::<Approvals>().unwrap().actions.is_empty());
    assert!(!action(
        &mut w,
        &ActionIntent {
            entity: approve,
            name: "stale".into()
        }
    ));
    assert!(drain_commands(&mut w).is_empty());
}

fn delayed_history_app() -> App {
    let mut app = crate::ui::app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    let row = proposal();
    let authority = row.proposal.request.owner_session.authority_id.to_string();
    app.world_mut().resource_mut::<crate::ui::Model>().unwrap().chat.hydrate(
        "parent".into(), authority, vec![
            serde_json::json!({"entry_id":"user","entry_seq":1,"role":"user","content":"Please review the layout."}),
            serde_json::json!({"entry_id":"assistant","entry_seq":2,"role":"assistant","content":"Your conversation stays here. Historical delegations are available through Earlier collaborations."})
        ], None).unwrap();
    for _ in 0..8 {
        app.update();
    }
    let timeline = app
        .world()
        .query::<Scrollbar>()
        .iter()
        .map(|(_, bar)| bar.target)
        .find(|root| crate::timeline::mounted_row(app.world(), *root, "parent:assistant").is_some())
        .unwrap();
    let before = app.world().get::<Rect>(timeline).copied().unwrap();
    let snapshot = app
        .world()
        .resource::<crate::ui::Model>()
        .unwrap()
        .chat
        .snapshot()
        .unwrap()
        .clone();
    // Match the reported order: history first, then a delayed completed-delegation inbox.
    let mut row = row;
    row.proposal.request.instructions =
        "Historical delegation instructions must not occupy the transcript.\n".repeat(200);
    completed(&mut row, PeerAssignmentOutcome::Completed);
    for _ in 0..3 {
        apply(app.world_mut(), inbox(vec![row.clone()]));
        for _ in 0..8 {
            app.update();
        }
        let after = app.world().get::<Rect>(timeline).unwrap();
        assert!(
            after.h > 300.,
            "history must retain usable height: {after:?}"
        );
        assert!((before.h - after.h).abs() < 1.);
        assert!(crate::timeline::mounted_row(app.world(), timeline, "parent:assistant").is_some());
        let current = app
            .world()
            .resource::<crate::ui::Model>()
            .unwrap()
            .chat
            .snapshot()
            .unwrap();
        assert_eq!(current.items, snapshot.items);
        assert!(
            app.world()
                .query::<Root>()
                .iter()
                .all(|(root, _)| children_of(app.world(), *root).is_empty())
        );
    }
    app
}
fn pending_history_app() -> App {
    let mut app = delayed_history_app();
    // The real historical chat had 95 proposals: 91 historical and four still
    // actionable, three approved-but-not-started. Completed-only fixtures miss it.
    let mut rows = vec![];
    for index in 0..95 {
        let mut row = proposal();
        row.proposal.proposal_id = format!("proposal-{index}");
        row.proposal.request.instructions = format!(
            "Request {index}\n{}",
            "Review this complete request without covering the actual conversation. ".repeat(60)
        );
        if index < 4 {
            row.binding = None;
            row.decision = None;
            if index < 3 {
                decision(&mut row, true);
            }
        } else {
            completed(&mut row, PeerAssignmentOutcome::Completed);
        }
        rows.push(row);
    }
    for _ in 0..3 {
        apply(app.world_mut(), inbox(rows.clone()));
        for _ in 0..8 {
            app.update();
        }
        assert_pending_layout(&app);
    }
    app
}
fn assert_pending_layout(app: &App) {
    let w = app.world();
    let root = w
        .query::<Root>()
        .iter()
        .find(|(e, _)| !children_of(w, *e).is_empty())
        .unwrap()
        .0;
    let inbox_rect = w.get::<Rect>(root).unwrap();
    assert!(
        inbox_rect.h <= 176.1,
        "inbox must remain bounded: {inbox_rect:?}"
    );
    assert!(
        w.get::<Scroll>(root).unwrap().content[1] > inbox_rect.h,
        "all requests stay scrollable"
    );
    let timeline = w
        .query::<Scrollbar>()
        .iter()
        .map(|(_, bar)| bar.target)
        .find(|root| crate::timeline::mounted_row(w, *root, "parent:assistant").is_some())
        .unwrap();
    assert!(
        w.get::<Rect>(timeline).unwrap().h > 200.,
        "actual conversation must stay visible"
    );
    assert_eq!(w.query::<OpenRequest>().iter().count(), 4);
    assert!(w.resource::<Approvals>().unwrap().actions.is_empty());
}
#[test]
fn pending_historical_requests_never_take_over_chat_even_when_expanded() {
    let mut app = pending_history_app();
    assert!(
        app.world()
            .query::<RequestDetails>()
            .iter()
            .all(|(_, d)| d.view.is_none())
    );
    let button = app.world().query::<OpenRequest>().iter().next().unwrap().0;
    assert!(activate(app.world_mut(), button));
    for _ in 0..8 {
        app.update();
    }
    assert!(
        drain_commands(app.world_mut()).is_empty(),
        "opening a thread does not authorize execution"
    );
    app.world_mut().insert_resource(Viewport::new(760., 650.));
    for _ in 0..8 {
        app.update();
    }
}

#[test]
#[ignore = "Synthetic pending-inbox Parley/GPU visual acceptance"]
fn pending_inbox_visual_acceptance() {
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::new());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    for (width, height, name) in [(1200, 850, "wide"), (760, 650, "narrow")] {
        let mut app = pending_history_app();
        app.world_mut().insert_resource(engine.clone());
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for open in [false, true] {
            if open {
                let node = app
                    .world()
                    .query::<RequestDetails>()
                    .iter()
                    .find(|(_, d)| d.text.contains("Request 0\n"))
                    .unwrap()
                    .0;
                let button = children_of(app.world(), node)[0];
                activate(app.world_mut(), button);
            }
            for _ in 0..12 {
                rndr_paint::frame(&mut app);
            }
            assert_pending_layout(&app);
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, width, height, 1.)
                    .unwrap()
            });
            image::save_buffer(
                format!("verification/native-pending-inbox-{name}-{open}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}

#[test]
fn delayed_completed_delegation_refresh_preserves_actual_chat_and_layout() {
    delayed_history_app();
}

#[test]
#[ignore = "Synthetic Parley/GPU delayed historical inbox acceptance"]
fn historical_inbox_visual_acceptance() {
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::new());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, name) in [(1200, 850, "wide"), (760, 650, "narrow")] {
        let mut app = delayed_history_app();
        app.world_mut().insert_resource(engine.clone());
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-historical-inbox-{name}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
#[ignore = "Local-only exported history/inbox probe; requires MEDOUSA_RELOAD_INPUT"]
fn recorded_historical_inbox_preserves_transcript() {
    let path = std::env::var("MEDOUSA_RELOAD_INPUT").expect("private diagnostic input path");
    let input: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let history = &input["history"];
    let mut app = crate::ui::app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    app.world_mut()
        .resource_mut::<crate::ui::Model>()
        .unwrap()
        .chat
        .hydrate(
            history["session_id"].as_str().unwrap().into(),
            history["authority_id"].as_str().unwrap().into(),
            history["turns"].as_array().unwrap().clone(),
            None,
        )
        .unwrap();
    for _ in 0..8 {
        app.update();
    }
    let snapshot = app
        .world()
        .resource::<crate::ui::Model>()
        .unwrap()
        .chat
        .snapshot()
        .unwrap()
        .clone();
    let page: PeerProposalInboxResponse = serde_json::from_value(input["inbox"].clone()).unwrap();
    apply(
        app.world_mut(),
        ApprovalUpdate {
            command: refresh(&snapshot.session_id),
            result: Ok(Payload::Inbox {
                budgets: Ok(vec![]),
                permissions: Ok(vec![]),
                proposals: Ok(page.proposals),
            }),
        },
    );
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(
        app.world()
            .resource::<crate::ui::Model>()
            .unwrap()
            .chat
            .snapshot()
            .unwrap()
            .items,
        snapshot.items
    );
    let w = app.world();
    assert!(
        w.query::<Root>()
            .iter()
            .all(|(e, _)| w.get::<Rect>(*e).unwrap().h <= 176.1)
    );
    assert_eq!(w.query::<OpenRequest>().iter().count(), 4);
    assert!(
        w.query::<RequestDetails>()
            .iter()
            .all(|(_, d)| d.view.is_none())
    );
    let timeline = w
        .query::<Scrollbar>()
        .iter()
        .map(|(_, bar)| bar.target)
        .find(|root| {
            snapshot
                .items
                .iter()
                .any(|item| crate::timeline::mounted_row(w, *root, &item.id).is_some())
        })
        .unwrap();
    assert!(w.get::<Rect>(timeline).unwrap().h > 300.);
}

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
