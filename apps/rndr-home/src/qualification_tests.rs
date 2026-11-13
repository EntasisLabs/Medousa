//! Synthetic qualification of the pinned rndr public APIs; no native input or daemon.
use super::*;

#[test]
fn framework_prompt_submit_is_still_end_of_batch_not_enter_snapshot() {
    // Minimal framework-only reproducer: no application submit guard or worker.
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    let field = app.world_mut().spawn((
        Layout::column(),
        Label::new("hi"),
        Prompt::new(),
        TextField,
        Focusable,
    ));
    app.update();
    assert!(request_focus(app.world_mut(), field));
    let end = "hi".len();
    app.world_mut().insert(
        field,
        Caret {
            index: end,
            anchor: end,
        },
    );
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![
        KeyEvent::named(NamedKey::Enter, Modifiers::new()),
        KeyEvent::character('!', Modifiers::new()),
    ];
    app.update();
    let submitted = events::<Submitted>(app.world());
    assert_eq!(submitted.len(), 1);
    assert_eq!(submitted[0].entity, field);
    assert_eq!(
        submitted[0].text, "hi!",
        "documents current framework limitation"
    );
}

#[test]
fn framework_prompt_reset_requires_marker_but_app_removes_it_synchronously() {
    let mut app = app();
    app.update();
    let field = app.world().resource::<Ui>().unwrap().composer;
    assert!(!app.world().has::<FieldTransactions>(field));
    assert!(!reset_field_draft(app.world_mut(), field, "direct reset"));
    reset_prompt(app.world_mut(), field, "first\nsecond");
    assert_eq!(
        app.world().get::<Label>(field).unwrap().text,
        "first\nsecond"
    );
    assert!(!app.world().has::<FieldTransactions>(field));
    reset_prompt(app.world_mut(), field, "");
    assert_eq!(app.world().get::<Label>(field).unwrap().text, "");
    assert_eq!(app.world().get::<Prompt>(field).unwrap().scroll, 0.);
    assert!(!app.world().has::<FieldTransactions>(field));
}

#[test]
fn parley_markdown_hugs_remeasures_and_retains_streaming_view() {
    let mut app = app();
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    app.world_mut().insert_resource(Viewport::new(1120., 820.));
    app.update();
    let ui = *app.world().resource::<Ui>().unwrap();
    let body =
        "A paragraph with **emphasis**, `code`, and words that wrap at the reading measure. "
            .repeat(8);
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .history_messages = vec![("assistant".into(), body.clone())];
    for _ in 0..8 {
        app.update();
    }
    let view = app.world().entities_with::<MarkdownView>()[0];
    let wide = *app.world().get::<Rect>(view).unwrap();
    assert!(wide.h > 32. && wide.w > 300.);
    // The existing Hug/no-inner-scroll adapter relies on the pinned 16pt inset.
    assert_eq!(app.world().get::<Layout>(view).unwrap().padding, 16.);
    assert_eq!(app.world().get::<Layout>(view).unwrap().height, Size::Hug);
    assert!(!app.world().has::<Scroll>(view));
    app.world_mut().insert_resource(Viewport::new(520., 820.));
    for _ in 0..8 {
        app.update();
    }
    let narrow = *app.world().get::<Rect>(view).unwrap();
    assert!(
        narrow.w < wide.w && narrow.h > wide.h,
        "wide={wide:?}, narrow={narrow:?}"
    );
    let suffix = "\n\n## Streaming suffix\n\nAnother **synthetic** paragraph.";
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .history_messages[0]
        .1
        .push_str(suffix);
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(app.world().entities_with::<MarkdownView>(), vec![view]);
    assert_eq!(
        app.world().get::<MarkdownView>(view).unwrap().source(),
        format!("{body}{suffix}")
    );
    assert!(app.world().get::<Rect>(view).unwrap().h > narrow.h);
    let mut scrolls = app.world().entities_with::<Scroll>();
    let desktop = app.world().resource::<desktop::Handles>().unwrap();
    scrolls.retain(|e| *e != desktop.primary_rail && *e != desktop.body);
    assert_eq!(
        scrolls,
        vec![ui.conversation],
        "transcript must retain its single scrollport"
    );
    assert!(conversation::copy_message(
        app.world_mut(),
        ui.conversation,
        "copy-message:0"
    ));
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().text,
        format!("{body}{suffix}")
    );
}

#[test]
fn lucide_atlas_has_nonempty_coverage_for_existing_control_candidates() {
    // Availability check only: deliberately does not redesign existing controls.
    let (width, height) = rndr_paint::icon_atlas_size();
    assert!(width > 0 && height > 0);
    for icon in [
        Icon::Plus,
        Icon::ArrowUp,
        Icon::ChevronDown,
        Icon::Copy,
        Icon::X,
    ] {
        let coverage = rndr_paint::icon_coverage(icon.scalar(), 16).unwrap();
        assert_eq!(coverage.len(), 256);
        assert!(coverage.iter().any(|a| *a > 0));
        assert_eq!(Icon::from_scalar(icon.scalar()), Some(icon));
    }
}
