use super::*;
use crate::ui_tests::{ready, reply};

fn activate(app: &mut App, action: &str) {
    let e = app
        .world()
        .entities_with::<Button>()
        .into_iter()
        .find(|e| app.world().get::<Button>(*e).unwrap().action == action)
        .unwrap_or_else(|| panic!("missing action {action}"));
    try_activate(app.world_mut(), e);
    app.update();
}
#[test]
fn model_menu_selects_exact_index_and_search_preserves_draft() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    {
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.models = vec![
            ("Zulu".into(), "alpha".into()),
            ("Alpha".into(), "omega".into()),
        ];
        m.selection = None;
    }
    reset_prompt(app.world_mut(), ui.composer, "unsent draft 👋");
    app.update();
    activate(&mut app, "model");
    assert!(
        app.world().resource::<Model>().unwrap().selection.is_none(),
        "opening isn't cycling or selecting"
    );
    let root = app
        .world()
        .resource::<menus::Menus>()
        .unwrap()
        .root
        .unwrap();
    let search = app
        .world()
        .entities_with::<TextField>()
        .into_iter()
        .find(|e| {
            *e != ui.composer && ancestor_with::<AnchoredPopup>(app.world(), *e) == Some(root)
        })
        .unwrap();
    app.world_mut().get_mut::<Label>(search).unwrap().text = "zulu".into();
    app.update();
    let actions = children_of(app.world(), root)
        .into_iter()
        .flat_map(|e| children_of(app.world(), e))
        .filter_map(|e| app.world().get::<Button>(e).map(|b| b.action.clone()))
        .collect::<Vec<_>>();
    assert!(actions.contains(&"pick:0".into()));
    assert!(!actions.contains(&"pick:1".into()));
    activate(&mut app, "pick:0");
    assert_eq!(app.world().resource::<Model>().unwrap().selection, Some(0));
    assert!(app.world().has::<Hidden>(root));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(ui.model)
    );
    assert_eq!(
        app.world().get::<Label>(ui.composer).unwrap().text,
        "unsent draft 👋"
    );
    assert!(rx.try_recv().is_err(), "menu clicks never submit");
}
#[test]
fn behavior_depth_reaches_exact_wire_request_without_mode_authority_override() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    activate(&mut app, "behavior");
    activate(&mut app, "behavior:depth");
    activate(&mut app, "depth:deep");
    activate(&mut app, "menu:close");
    assert!(rx.try_recv().is_err());
    reset_prompt(app.world_mut(), ui.composer, "question");
    app.update();
    activate(&mut app, "send");
    let Command::Send(r) = rx.try_recv().unwrap().command else {
        panic!()
    };
    assert_eq!(r.response_depth_mode, "deep");
    assert!(r.agent_mode.is_none(), "session mode remains daemon-owned");
    assert!(!r.code_project_setup_authorized);
}
#[test]
fn runtime_menu_does_not_enable_unimplemented_external_agents() {
    let (mut app, mut rx) = ready();
    activate(&mut app, "runtime");
    let external = app
        .world()
        .entities_with::<Button>()
        .into_iter()
        .filter(|e| app.world().get::<Button>(*e).unwrap().action == "unsupported")
        .collect::<Vec<_>>();
    assert_eq!(external.len(), 8);
    for e in external {
        assert!(!app.world().get::<Button>(e).unwrap().enabled);
        assert_eq!(try_activate(app.world_mut(), e), ActivationResult::Rejected);
    }
    assert!(rx.try_recv().is_err());
}
#[test]
fn composer_hugs_wraps_caps_and_shrinks() {
    let (mut app, _) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    for _ in 0..3 {
        app.update();
    }
    let empty = app.world().get::<Rect>(ui.composer).unwrap().h;
    assert!(empty <= 40., "empty composer {empty}");
    reset_prompt(app.world_mut(), ui.composer, &"wrapped words ".repeat(300));
    for _ in 0..3 {
        app.update();
    }
    let large = app.world().get::<Rect>(ui.composer).unwrap().h;
    assert!(large > empty * 2.);
    assert!(large < 180., "six-line cap {large}");
    reset_prompt(app.world_mut(), ui.composer, "");
    for _ in 0..3 {
        app.update();
    }
    assert!((app.world().get::<Rect>(ui.composer).unwrap().h - empty).abs() < 1.);
}
#[test]
fn mode_action_waits_for_daemon_and_stale_mode_reply_is_ignored() {
    let (mut app, mut rx) = ready();
    {
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.modes = vec![
            ("general".into(), "General".into(), true),
            ("teacher".into(), "Teacher".into(), true),
            ("coder".into(), "Coder".into(), false),
        ];
        m.mode = Some("general".into());
    }
    activate(&mut app, "mode");
    activate(&mut app, "mode:teacher");
    let request = rx.try_recv().unwrap();
    assert!(
        matches!(request.command, Command::SetMode(ref s, medousa_types::AgentModeId::Teacher) if s == "s")
    );
    assert_eq!(
        app.world().resource::<Model>().unwrap().mode.as_deref(),
        Some("general")
    );
    assert!(app.world().resource::<Model>().unwrap().loading);
    reply(&mut app, request.generation, Update::Mode("teacher".into()));
    assert_eq!(
        app.world().resource::<Model>().unwrap().mode.as_deref(),
        Some("teacher")
    );
    assert!(!app.world().resource::<Model>().unwrap().loading);
    app.world_mut().resource_mut::<Model>().unwrap().generation += 1;
    reply(&mut app, request.generation, Update::Mode("general".into()));
    assert_eq!(
        app.world().resource::<Model>().unwrap().mode.as_deref(),
        Some("teacher")
    );
}
fn click(app: &mut App, position: [f32; 2]) {
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position,
        pressed: true,
        released: true,
        press_at: Some(position),
        ..Default::default()
    };
    app.update();
}
#[test]
fn physical_pointer_opens_menu_selects_model_and_outside_click_dismisses() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    let rect = *app.world().get::<Rect>(ui.model).unwrap();
    click(&mut app, [rect.x + 10., rect.y + 10.]);
    let root = app
        .world()
        .resource::<menus::Menus>()
        .unwrap()
        .root
        .unwrap();
    assert!(!app.world().has::<Hidden>(root));
    for _ in 0..2 {
        app.update();
    }
    let option = app
        .world()
        .entities_with::<Button>()
        .into_iter()
        .find(|e| app.world().get::<Button>(*e).unwrap().action == "pick:0")
        .unwrap();
    let r = *app.world().get::<Rect>(option).unwrap();
    click(&mut app, [r.x + 10., r.y + 10.]);
    assert!(app.world().has::<Hidden>(root));
    click(&mut app, [rect.x + 10., rect.y + 10.]);
    let root = app
        .world()
        .resource::<menus::Menus>()
        .unwrap()
        .root
        .unwrap();
    click(&mut app, [30., 200.]);
    assert!(app.world().has::<Hidden>(root));
    assert!(rx.try_recv().is_err());
}
#[test]
fn copy_click_uses_current_assistant_text_and_cannot_copy_user_as_assistant() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .history_messages = vec![
        ("user".into(), "my draft".into()),
        (
            "assistant".into(),
            "**answer**\\n\\n```sh\\necho hello\\n```".into(),
        ),
    ];
    app.update();
    activate(&mut app, "copy-message:1");
    let clipboard = app.world().resource::<Clipboard>().unwrap();
    assert_eq!(clipboard.text, "**answer**\\n\\n```sh\\necho hello\\n```");
    assert!(clipboard.dirty);
    assert!(!conversation::copy_message(
        app.world_mut(),
        ui.conversation,
        "copy-message:0"
    ));
    assert!(!conversation::copy_message(
        app.world_mut(),
        ui.conversation,
        "copy-message:99"
    ));
    assert!(rx.try_recv().is_err());
}
#[test]
fn escape_closes_popup_and_restores_focus_without_sending() {
    let (mut app, mut rx) = ready();
    let ui = *app.world().resource::<Ui>().unwrap();
    activate(&mut app, "model");
    let root = app
        .world()
        .resource::<menus::Menus>()
        .unwrap()
        .root
        .unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Escape, Modifiers::new())];
    app.update();
    assert!(app.world().has::<Hidden>(root));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(ui.model)
    );
    assert!(rx.try_recv().is_err());
}
