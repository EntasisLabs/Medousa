mod backend;
mod conversation;
#[cfg(test)]
mod interaction_tests;
mod menus;
#[cfg(test)]
mod ui_tests;
use backend::{Command, Reply, Request, Update};
use medousa_rndr_home::{Chat, turn_request};
use rndr_app::{button, child, column, label};
use rndr_core::*;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc;

#[cfg(feature = "observe")]
struct FixtureObservation;

#[derive(Default)]
struct SubmitBoundary {
    safe: bool,
}
struct Model {
    chat: Chat,
    commands: Option<mpsc::UnboundedSender<Request>>,
    models: Vec<(String, String)>,
    selection: Option<usize>,
    loading: bool,
    generation: u64,
    // Memory only. Never reconcile by matching prose: identical prompts are valid.
    recovery: Vec<(String, String)>,
    pending_recovery: Option<usize>,
    history_messages: Vec<(String, String)>,
    mode: Option<String>,
    modes: Vec<(String, String, bool)>,
    depth: String,
}
#[derive(Clone, Copy)]
struct Ui {
    empty: Entity,
    composer: Entity,
    status: Entity,
    model: Entity,
    center: Entity,
    send: Entity,
    cancel: Entity,
    recover: Entity,
    runtime: Entity,
    mode: Entity,
    behavior: Entity,
    conversation: Entity,
}
const MUTED: Color = Color::rgb(0.53, 0.53, 0.55);
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut origin = "http://127.0.0.1:9777".to_string();
    let mut data_dir: Option<PathBuf> = None;
    let mut session = None;
    let mut export: Option<PathBuf> = None;
    let mut offline = false;
    let mut observe_offline = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--daemon" => origin = args.next().ok_or("--daemon needs an origin")?,
            "--data-dir" => {
                data_dir = Some(args.next().ok_or("--data-dir needs a directory")?.into())
            }
            "--session" => session = Some(args.next().ok_or("--session needs an id")?),
            "--export" => {
                export = Some(args.next().ok_or("--export needs a directory")?.into());
                offline = true;
            }
            "--offline" => offline = true,
            "--observe-offline" => {
                offline = true;
                observe_offline = true;
            }
            "--help" | "-h" => {
                println!(
                    "Medousa (rndr)\n  --data-dir PATH [--daemon http://127.0.0.1:9777] [--session ID]\n  --offline  No credentials or network\n  --export DIRECTORY  Offline GPU PNG evidence; no network\n  --observe-offline  Synthetic live window capture, 15s normal exit (requires feature observe)\nUse the existing app to start/configure the daemon. Sending can incur inference charges."
                );
                return Ok(());
            }
            _ if arg.starts_with("-psn_") => {}
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let mut app = app();
    let mut _feed = None;
    if !offline {
        medousa_rndr_home::local_url(&origin)?;
        let dir = data_dir.ok_or("--data-dir is required; use --offline for UI only. No credentials are discovered or created automatically.")?;
        let (tx, rx) = mpsc::unbounded_channel();
        app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx.clone());
        _feed = Some(feed::spawn_thread(
            app.world_mut(),
            FeedConfig::queue(256),
            move |out| backend::run(origin, dir, rx, out),
        ));
        if let Some(id) = session {
            app.world_mut().resource_mut::<Model>().unwrap().loading = true;
            tx.send(Request {
                generation: 0,
                command: Command::Open(id),
            })?;
        }
    }
    #[cfg(feature = "observe")]
    let _exit_timer = if observe_offline {
        // No data-dir/session/daemon data can enter this branch: offline=true
        // before any worker is created. Only the audited empty fixture is public.
        use rndr_render::observe::{ObservationContent, ObservationSession};
        app.world_mut().insert_resource(FixtureObservation);
        app.world_mut().insert_resource(ObservationContent::Deny);
        let descriptor = ObservationSession::enable(
            app.world_mut(),
            "Medousa offline empty fixture · dark · parley",
        )?;
        eprintln!("observation descriptor: {}", descriptor.display());
        #[derive(Clone)]
        struct Exit;
        impl Event for Exit {}
        app.add_system(Schedule::Update, |world| {
            if !events::<Exit>(world).is_empty() {
                world.resource_mut::<Input>().unwrap().quit = true;
            }
        });
        Some(after(
            app.world_mut(),
            std::time::Duration::from_secs(15),
            Exit,
        ))
    } else {
        None
    };
    #[cfg(not(feature = "observe"))]
    if observe_offline {
        return Err("--observe-offline requires --features observe".into());
    }
    if let Some(dir) = export {
        std::fs::create_dir_all(&dir)?;
        app.world_mut()
            .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
        let mut exporter = rndr_render::Exporter::new()?;
        for (name, width, height, transcript) in [
            ("empty", 1120., 820., false),
            ("chat", 1120., 820., true),
            ("narrow", 520., 760., true),
            ("draft", 1120., 820., true),
            ("models", 1120., 820., true),
            ("behavior", 1120., 820., true),
            ("depth", 1120., 820., true),
            ("runtime", 1120., 820., true),
        ] {
            app.world_mut()
                .insert_resource(Viewport::new(width, height));
            app.update();
            menus::close(app.world_mut());
            let ui = *app.world().resource::<Ui>().unwrap();
            reset_prompt(app.world_mut(), ui.composer, "");
            {
                let m = app.world_mut().resource_mut::<Model>().unwrap();
                m.models = vec![
                    ("ChatGPT · synthetic".into(), "GPT-6.1 Sol".into()),
                    ("Anthropic · synthetic".into(), "Claude · example".into()),
                    ("Local · synthetic".into(), "Local model · example".into()),
                ];
                m.selection = Some(0);
            }
            if transcript {
                let model = app.world_mut().resource_mut::<Model>().unwrap();
                model.chat.transcript = "### You\n\nCan we make this a native chat?\n\n### Medousa\n\nYes — this is **rndr**, not a webview.\n\n- Real session history\n- Streaming chronological text\n- `Shift+Enter` for a new line\n\nThis is an offline visual fixture; no inference was sent.".into();
            }
            if transcript {
                app.world_mut().resource_mut::<Model>().unwrap().history_messages = vec![
                    ("user".into(), "Can we make this feel more like the current chat?".into()),
                    ("assistant".into(), "Yes — a conversation, not a document.\n\nThe composer now **grows with your draft**, and the model button opens a searchable menu instead of cycling.\n\n```sh\ncargo run --manifest-path apps/rndr-home/Cargo.toml -- --offline\n```\n\nMode changes use the same daemon contract as the Svelte app. Behavior supports next-turn response depth; external runtimes and the remaining controls are explicitly marked as not ported.\n\nThis is synthetic offline visual evidence. **No inference was sent.**".into()),
                ];
            }
            if name == "draft" {
                reset_prompt(
                    app.world_mut(),
                    ui.composer,
                    "A longer draft should grow naturally.\nShift+Enter adds another line.\nThe transcript keeps the remaining space.\nAnd the editor is capped at six visual lines.\nFive\nSix\nSeven — internally scrollable",
                );
            }
            for _ in 0..5 {
                app.update();
            }
            match name {
                "models" => menus::open(app.world_mut(), menus::Kind::Model, ui.model),
                "behavior" => menus::open(app.world_mut(), menus::Kind::Behavior, ui.behavior),
                "runtime" => menus::open(app.world_mut(), menus::Kind::Runtime, ui.runtime),
                "depth" => menus::open(app.world_mut(), menus::Kind::Depth, ui.behavior),
                _ => {}
            }
            for _ in 0..4 {
                app.update();
            }
            let frame = rndr_paint::frame(&mut app);
            let image = exporter.render_frame(
                Some(app.world()),
                &frame,
                width as u32,
                height as u32,
                1.,
            )?;
            std::fs::write(
                dir.join(format!("{name}.png")),
                rndr_render::encode_png(&image)?,
            )?;
        }
    } else {
        rndr_render::run(app);
    }
    Ok(())
}
fn app() -> App {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "Medousa · rndr preview".into(),
        size: LogicalSize::new(1120., 820.),
        min_size: Some(LogicalSize::new(480., 650.)),
        ..Default::default()
    });
    app.world_mut().insert_resource(Theme {
        canvas: Color::rgb(0.055, 0.055, 0.06),
        pane: Color::rgb(0.12, 0.12, 0.13),
        title: Color::rgb(0.15, 0.15, 0.16),
        body: Color::rgb(0.09, 0.09, 0.10),
        text: Color::rgb(0.9, 0.89, 0.87),
        radius: 14.,
        ..Default::default()
    });
    app.world_mut().insert_resource(Model {
        chat: Chat::offline(),
        commands: None,
        models: vec![],
        selection: None,
        loading: false,
        generation: 0,
        recovery: Vec::new(),
        pending_recovery: None,
        history_messages: Vec::new(),
        mode: None,
        modes: Vec::new(),
        depth: "standard".into(),
    });
    app.add_plugin(rndr_app::FormNavigation);
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(SubmitBoundary::default());
    app.world_mut().insert_resource(menus::Menus::default());
    app.add_system_front(Schedule::Input, guard_submit_boundary);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, update);
    app
}
fn row(world: &mut World, parent: Entity, height: Size) -> Entity {
    let e = world.spawn((Layout {
        width: Size::Fill,
        height,
        gap: 10.,
        ..Layout::row()
    },));
    child(world, parent, e)
}
fn muted(world: &mut World, parent: Entity, text: &str) -> Entity {
    let e = label(world, parent, text, false);
    world.get_mut::<Label>(e).unwrap().color = Some(MUTED);
    world.get_mut::<TextStyle>(e).unwrap().size = 12.;
    e
}
fn setup(world: &mut World) {
    let theme = theme_of(world);
    let root = world.spawn((
        Layout {
            padding: 12.,
            gap: 8.,
            ..column(Size::Fill)
        },
        theme.canvas,
    ));
    world.insert(root, Radius(14.));
    world.insert(
        root,
        Border {
            color: Color::rgb(0.2, 0.2, 0.21),
            width: 1.,
        },
    );
    let tabs = row(world, root, Size::Px(38.));
    let tab = button(world, tabs, "reload", "Today");
    world.get_mut::<Layout>(tab).unwrap().width = Size::Px(125.);
    let add = button(world, tabs, "new", "+");
    world.get_mut::<Layout>(add).unwrap().width = Size::Px(42.);
    muted(world, tabs, "Medousa · native preview");
    let reload = button(world, tabs, "reload", "↻");
    world.get_mut::<Layout>(reload).unwrap().width = Size::Px(36.);
    world.insert(
        reload,
        Semantics {
            name: Some("Reload history".into()),
            ..Default::default()
        },
    );
    let content = world.spawn((Layout {
        gap: 0.,
        ..column(Size::Fill)
    },));
    child(world, root, content);
    let empty = world.spawn((Layout {
        gap: 10.,
        ..column(Size::Fill)
    },));
    child(world, content, empty);
    let top_air = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(90.),
        ..Layout::column()
    },));
    child(world, empty, top_air);
    let spacer = world.spawn((column(Size::Fill),));
    child(world, empty, spacer);
    let heading = label(world, empty, "What are we doing today?", true);
    world.get_mut::<TextStyle>(heading).unwrap().align = TextAlign::Center;
    world.get_mut::<TextStyle>(heading).unwrap().size = 21.;
    let space = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(30.),
        ..Layout::column()
    },));
    child(world, empty, space);
    let recent = button(world, empty, "recent", "Continue where we left off");
    world.insert(recent, Color::rgba(0., 0., 0., 0.));
    world.get_mut::<TextStyle>(recent).unwrap().align = TextAlign::Center;
    world.get_mut::<TextStyle>(recent).unwrap().size = 14.;
    world.get_mut::<Label>(recent).unwrap().color = Some(MUTED);
    let mut underline = TextSpan::new(0, "Continue where we left off".len(), MUTED);
    underline.underline = true;
    world.insert(
        recent,
        Spans {
            runs: vec![underline],
        },
    );
    let spacer = world.spawn((column(Size::Fill),));
    child(world, empty, spacer);
    let conversation = conversation::setup(world, content);
    world.insert(conversation, Hidden);
    let centered = row(world, root, Size::Hug);
    let left = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, centered, left);
    let center = world.spawn((Layout {
        width: Size::Px(670.),
        height: Size::Hug,
        gap: 5.,
        ..Layout::column()
    },));
    child(world, centered, center);
    let right = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, centered, right);
    let stickies = muted(world, center, "Sticky notes · not ported");
    world.get_mut::<TextStyle>(stickies).unwrap().align = TextAlign::End;
    let box_ = world.spawn((
        Layout {
            padding: 10.,
            gap: 4.,
            ..column(Size::Hug)
        },
        theme.pane,
        Radius(18.),
        Border {
            color: Color::rgb(0.2, 0.2, 0.21),
            width: 1.,
        },
    ));
    child(world, center, box_);
    let composer = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            min_height: 28.,
            ..Layout::column()
        },
        Label::new(""),
        Prompt::new().max_lines(6),
        TextField,
        Focusable,
    ));
    world.insert(composer, Placeholder::new("Message Medousa…"));
    world.insert(
        composer,
        TextStyle {
            family: "sans-serif".into(),
            size: 16.,
            ..Default::default()
        },
    );
    world.insert(
        composer,
        Semantics {
            name: Some("Message · Enter to send, Shift Enter for new line".into()),
            ..Default::default()
        },
    );
    child(world, box_, composer);
    let controls = row(world, box_, Size::Px(34.));
    let plus = button(world, controls, "attachments", "+");
    world.get_mut::<Layout>(plus).unwrap().width = Size::Px(28.);
    world.insert(plus, Color::rgba(0., 0., 0., 0.));
    world.insert(
        plus,
        Semantics {
            name: Some("Attachments · not ported".into()),
            ..Default::default()
        },
    );
    let model = button(world, controls, "model", "Select model");
    world.get_mut::<Layout>(model).unwrap().width = Size::Fill;
    world.insert(model, Color::rgba(0., 0., 0., 0.));
    world.get_mut::<TextStyle>(model).unwrap().size = 13.;
    let model_air = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, controls, model_air);
    let send = button(world, controls, "send", "↑");
    world.get_mut::<Layout>(send).unwrap().width = Size::Px(32.);
    world.insert(
        send,
        Semantics {
            name: Some("Send message · may incur inference charges".into()),
            ..Default::default()
        },
    );
    world.insert(send, Color::rgb(0.85, 0.84, 0.82));
    world.get_mut::<Label>(send).unwrap().color = Some(theme.canvas);
    world.insert(send, Radius(16.));
    let cancel = button(world, controls, "cancel", "Stop");
    world.get_mut::<Layout>(cancel).unwrap().width = Size::Px(58.);
    for e in [model, plus, send, cancel] {
        let layout = world.get_mut::<Layout>(e).unwrap();
        layout.min_height = 0.;
        layout.height = Size::Px(32.);
    }
    world.get_mut::<TextStyle>(send).unwrap().align = TextAlign::Center;
    world.get_mut::<Label>(model).unwrap().overflow = TextOverflow::Ellipsis;
    let settings = row(world, center, Size::Px(28.));
    let runtime = button(world, settings, "runtime", "Medousa v");
    let mode = button(world, settings, "mode", "Mode v");
    let air = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, settings, air);
    let behavior = button(world, settings, "behavior", "Behavior v");
    for (e, width) in [(runtime, 110.), (mode, 110.), (behavior, 105.)] {
        world.get_mut::<Layout>(e).unwrap().min_height = 0.;
        world.get_mut::<Layout>(e).unwrap().height = Size::Px(28.);
        world.insert(e, Color::rgba(0., 0., 0., 0.));
        world.get_mut::<Layout>(e).unwrap().width = Size::Px(width);
        world.get_mut::<TextStyle>(e).unwrap().size = 12.;
        world.get_mut::<Label>(e).unwrap().color = Some(MUTED);
    }
    let bottom_space = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(4.),
        ..Layout::column()
    },));
    child(world, root, bottom_space);
    let recover = button(
        world,
        root,
        "recover",
        "Restore last unconfirmed prompt · empty composer only",
    );
    world.insert(recover, Hidden);
    let status = muted(world, root, "Offline preview");
    world.insert_resource(Ui {
        empty,
        composer,
        status,
        model,
        center,
        send,
        cancel,
        recover,
        runtime,
        mode,
        behavior,
        conversation,
    });
    request_focus(world, composer);
}
// rndr's reset_field_draft requires a transactional TextField opt-in. Keep
// that marker only for this synchronous reset: leaving it on a Prompt would
// route Enter/Shift+Enter through the single-line transaction dispatcher.
fn reset_prompt(world: &mut World, entity: Entity, text: &str) {
    world.insert(entity, FieldTransactions(|_, _, _| {}));
    assert!(reset_field_draft(world, entity, text));
    world.remove::<FieldTransactions>(entity);
    if let Some(prompt) = world.get_mut::<Prompt>(entity) {
        prompt.scroll = 0.;
    }
}
fn dispatch(world: &mut World, command: Command) {
    let model = world.resource_mut::<Model>().unwrap();
    if let Some(tx) = &model.commands {
        if tx
            .send(Request {
                generation: model.generation,
                command,
            })
            .is_err()
        {
            model
                .chat
                .fail("Connection worker unavailable. Restart after fixing daemon/auth.".into());
            model.loading = false;
            model.commands = None;
        }
    } else {
        model.chat.status = "Offline preview · restart with --data-dir to connect".into();
        model.loading = false;
    }
}
// Prompt currently emits Submitted after folding the entire key batch rather
// than at the Enter boundary. Refuse ambiguous batches: never send enlarged
// inference accidentally. Ordinary Enter-at-end still uses native Prompt.
fn guard_submit_boundary(world: &mut World) {
    let input = world.resource::<Input>().unwrap();
    let presses = input
        .keys
        .iter()
        .filter(|k| !k.released && !k.synthetic)
        .collect::<Vec<_>>();
    let enter = presses
        .iter()
        .position(|k| k.logical == LogicalKey::Named(NamedKey::Enter) && !k.modifiers.shift);
    let safe = enter.is_none_or(|i| i + 1 == presses.len())
        && input.ime_preedit.is_empty()
        && input.ime_commit.is_empty()
        && !(input.keys.is_empty() && input.key == Some(Key::Enter) && !input.chars.is_empty());
    world.resource_mut::<SubmitBoundary>().unwrap().safe = safe;
}
fn update(world: &mut World) {
    let ui = *world.resource::<Ui>().unwrap();
    let replies = events::<Reply>(world).to_vec();
    for reply in replies {
        let model = world.resource_mut::<Model>().unwrap();
        if reply.generation != model.generation
            && !matches!(
                reply.update,
                Update::Models(_) | Update::Modes(_) | Update::Unavailable(_)
            )
        {
            continue;
        }
        match reply.update {
            Update::History(session, transcript, active) => {
                model.chat = Chat::history(session, transcript, active.is_some());
                if let Some(turn) = active {
                    model.chat.resume(turn.turn_id);
                }
                model.loading = false;
            }
            Update::SettledHistory(session, transcript) => {
                let status = model
                    .chat
                    .status
                    .trim_end_matches(" · reconciling durable history…")
                    .to_string();
                model.chat = Chat::history(session, transcript, false);
                model.chat.status = format!("{status} · history reconciled (non-atomic reads)");
                model.loading = false;
            }
            Update::Messages(messages) => {
                model.history_messages = messages;
            }
            Update::Mode(mode) => {
                model.mode = Some(mode);
                model.loading = false;
            }
            Update::Modes(modes) => {
                model.modes = modes;
            }
            Update::Admitted(turn) => {
                model.chat.admitted(turn);
                if let Some(index) = model.pending_recovery.take() {
                    model.recovery.remove(index);
                }
            }
            Update::Stream(e) => {
                let terminal = e.event.is_terminal();
                model.chat.apply(e);
                if terminal {
                    model.chat.uncertain = true;
                    model
                        .chat
                        .status
                        .push_str(" · reconciling durable history…");
                }
            }
            Update::Models(models) => {
                model.models = models;
                model.chat.status =
                    "Connected · create + or continue a chat; select model before sending".into();
            }
            Update::Notice(message) => {
                model.chat.status = message;
                model.loading = false;
            }
            Update::Failure(message) => {
                model.chat.fail(message);
                model.loading = false;
            }
            Update::Unavailable(message) => {
                model.chat.status = message;
                model.chat.busy = false;
                model.loading = false;
                model.commands = None;
            }
        }
    }
    let mut actions = take_action_intents(world)
        .into_iter()
        .map(|a| (a.name, None))
        .collect::<Vec<_>>();
    if world.resource::<SubmitBoundary>().unwrap().safe {
        actions.extend(
            events::<Submitted>(world)
                .iter()
                .filter(|s| s.entity == ui.composer)
                .map(|s| ("send".into(), Some(s.text.clone()))),
        );
    } else if events::<Submitted>(world)
        .iter()
        .any(|s| s.entity == ui.composer)
    {
        world.resource_mut::<Model>().unwrap().chat.status = "Input changed in the Enter batch; nothing sent, draft preserved. Press Send or Enter again after composition finishes.".into();
    }
    for (action, submitted) in actions {
        let (busy, loading) = {
            let m = world.resource::<Model>().unwrap();
            (m.chat.busy, m.loading)
        };
        match action.as_str() {
            "new" | "recent" | "reload" if !busy && !loading => {
                let command = match action.as_str() {
                    "new" => Command::New,
                    "recent" => Command::Recent,
                    _ => {
                        let Some(s) = world.resource::<Model>().unwrap().chat.session.clone()
                        else {
                            continue;
                        };
                        Command::Open(s)
                    }
                };
                let m = world.resource_mut::<Model>().unwrap();
                m.loading = true;
                m.mode = None;
                m.generation += 1;
                menus::close(world);
                dispatch(world, command);
            }
            "model" if !busy => menus::open(world, menus::Kind::Model, ui.model),
            "runtime" if !busy => menus::open(world, menus::Kind::Runtime, ui.runtime),
            "mode" if !busy && !loading => menus::open(world, menus::Kind::Mode, ui.mode),
            "behavior" if !busy => menus::open(world, menus::Kind::Behavior, ui.behavior),
            _ if action.starts_with("copy-message:") => {
                conversation::copy_message(world, ui.conversation, &action);
            }
            "menu:close" => menus::close(world),
            "behavior:depth" if !busy => menus::open(world, menus::Kind::Depth, ui.behavior),
            "behavior:main" if !busy => menus::open(world, menus::Kind::Behavior, ui.behavior),
            _ if action.starts_with("pick:") && !busy => {
                if let Ok(index) = action[5..].parse::<usize>() {
                    let m = world.resource_mut::<Model>().unwrap();
                    if index < m.models.len() {
                        m.selection = Some(index);
                    }
                }
                menus::close(world);
            }
            _ if action.starts_with("depth:") && !busy => {
                let depth = &action[6..];
                if ["concise", "standard", "deep"].contains(&depth) {
                    world.resource_mut::<Model>().unwrap().depth = depth.into();
                }
                menus::open(world, menus::Kind::Behavior, ui.behavior);
            }
            _ if action.starts_with("mode:") && !busy && !loading => {
                let id = &action[5..];
                let m = world.resource::<Model>().unwrap();
                if m.modes.iter().any(|(mode, _, ok)| mode == id && *ok)
                    && let (Some(session), Ok(mode)) = (
                        m.chat.session.clone(),
                        serde_json::from_value(serde_json::Value::String(id.into())),
                    )
                {
                    menus::close(world);
                    world.resource_mut::<Model>().unwrap().loading = true;
                    dispatch(world, Command::SetMode(session, mode));
                }
            }
            "send" if !loading => {
                let prompt = submitted
                    .unwrap_or_else(|| world.get::<Label>(ui.composer).unwrap().text.clone());
                let m = world.resource_mut::<Model>().unwrap();
                if m.commands.is_none() {
                    m.chat.status = "Offline preview · nothing sent".into();
                    continue;
                }
                let Some(index) = m.selection else {
                    m.chat.status =
                        "Choose a model from the menu first; sending may incur charges".into();
                    continue;
                };
                let (provider, model) = m.models[index].clone();
                match m.chat.begin(&prompt) {
                    Ok(()) => {
                        m.generation += 1;
                        m.pending_recovery = Some(m.recovery.len());
                        m.recovery
                            .push((m.chat.session.clone().unwrap(), prompt.clone()));
                        let mut request = turn_request(
                            m.chat.session.as_deref().unwrap(),
                            &prompt,
                            &provider,
                            &model,
                        );
                        request.response_depth_mode = m.depth.clone();
                        menus::close(world);
                        dispatch(world, Command::Send(Box::new(request)));
                        // Prompt emits a snapshot, not a synchronous owner callback.
                        // Never erase edits queued after that submission boundary.
                        if world.resource::<Model>().unwrap().chat.busy
                            && world.get::<Label>(ui.composer).unwrap().text == prompt
                        {
                            reset_prompt(world, ui.composer, "");
                            request_focus(world, ui.composer);
                        }
                    }
                    Err(e) => m.chat.status = e.into(),
                }
            }
            "recover"
                if !busy
                    && !loading
                    && world.get::<Label>(ui.composer).unwrap().text.is_empty() =>
            {
                let m = world.resource_mut::<Model>().unwrap();
                if let Some((_, prompt)) = m.recovery.pop() {
                    m.pending_recovery = None;
                    reset_prompt(world, ui.composer, &prompt);
                    request_focus(world, ui.composer);
                }
            }
            "cancel" if !loading && (busy || world.resource::<Model>().unwrap().chat.uncertain) => {
                if let Some(id) = world.resource::<Model>().unwrap().chat.session.clone() {
                    dispatch(world, Command::Cancel(id));
                }
            }
            _ => {}
        }
    }
    let m = world.resource::<Model>().unwrap();
    let status = if m.loading {
        "Loading history…".into()
    } else {
        m.chat.status.clone()
    };
    let status = m
        .chat
        .session
        .as_ref()
        .map_or_else(|| status.clone(), |id| format!("Local · {id} · {status}"));
    let selected = m
        .selection
        .filter(|i| *i < m.models.len())
        .map(|i| format!("{} · {} v", m.models[i].1, m.models[i].0))
        .unwrap_or_else(|| "Select model v".into());
    let mode_label = m
        .mode
        .as_ref()
        .and_then(|id| m.modes.iter().find(|(value, _, _)| value == id))
        .map(|(_, label, _)| format!("{label} v"))
        .unwrap_or_else(|| "Mode v".into());
    let mut messages = m.history_messages.clone();
    messages.extend(m.chat.live_messages.clone());
    if !m.chat.busy {
        for (session, prompt) in &m.recovery {
            messages.push(("notice".into(), format!("Recoverable prompt · {session}\n\nAdmission not confirmed. Check history before restoring.\n\n{prompt}")));
        }
    }
    let busy = m.chat.busy;
    let uncertain = m.chat.uncertain;
    let connected = m.commands.is_some();
    if messages.is_empty() {
        world.insert(ui.conversation, Hidden);
        world.remove::<Hidden>(ui.empty);
    } else {
        world.remove::<Hidden>(ui.conversation);
        world.insert(ui.empty, Hidden);
    }
    conversation::sync(world, ui.conversation, &messages);
    menus::update(world);
    world.get_mut::<Label>(ui.mode).unwrap().text = mode_label;
    world.get_mut::<Label>(ui.status).unwrap().text = status;
    world.get_mut::<Label>(ui.model).unwrap().text = selected;
    world.get_mut::<Label>(ui.send).unwrap().text = if busy { "…" } else { "↑" }.into();
    if busy || uncertain {
        world.remove::<Hidden>(ui.cancel);
    } else {
        world.insert(ui.cancel, Hidden);
    }
    // Keep composition below ~670 px and shrink with the actual viewport.
    let width = world.resource::<Viewport>().unwrap().width;
    let center_width = (width - 48.).clamp(250., 670.);
    world.get_mut::<Layout>(ui.center).unwrap().width = Size::Px(center_width);
    world.get_mut::<Layout>(ui.model).unwrap().width =
        Size::Px((center_width - 136.).clamp(100., 340.));
    let m = world.resource::<Model>().unwrap();
    let has_session = m.chat.session.is_some();
    let can_send = connected
        && !busy
        && !uncertain
        && !m.loading
        && has_session
        && m.selection.is_some()
        && !world
            .get::<Label>(ui.composer)
            .unwrap()
            .text
            .trim()
            .is_empty();
    let can_navigate = connected && !busy && !m.loading;
    let can_settings = !busy && !m.loading;
    let can_select = !busy && !m.models.is_empty();
    let can_cancel = connected && !m.loading && has_session && (busy || uncertain);
    let has_recovery = !m.recovery.is_empty();
    let can_recover = has_recovery
        && !busy
        && !m.loading
        && world.get::<Label>(ui.composer).unwrap().text.is_empty();
    if has_recovery {
        world.remove::<Hidden>(ui.recover);
    } else {
        world.insert(ui.recover, Hidden);
    }
    for e in world.entities_with::<Button>() {
        let b = world.get_mut::<Button>(e).unwrap();
        b.enabled = match b.action.as_str() {
            "send" => can_send,
            "model" => !busy,
            "runtime" | "behavior" => can_settings,
            "mode" => can_settings && has_session && connected,
            "attachments" => false,
            _ if b.action.starts_with("pick:") => can_select,
            "new" | "recent" => can_navigate,
            "reload" => can_navigate && has_session,
            "cancel" => can_cancel,
            "recover" => can_recover,
            _ => b.enabled,
        };
    }
    let theme = theme_of(world);
    world.insert(
        ui.send,
        if can_send {
            Color::rgb(0.85, 0.84, 0.82)
        } else {
            theme.pane
        },
    );
    world.get_mut::<Label>(ui.send).unwrap().color =
        Some(if can_send { theme.canvas } else { MUTED });
    #[cfg(feature = "observe")]
    if world.has_resource::<FixtureObservation>() {
        use rndr_render::observe::ObservationContent;
        // Explicit fixture opt-in must not expose text someone types into the
        // native window while a read-only observer is attached.
        let m = world.resource::<Model>().unwrap();
        let public = m.commands.is_none()
            && m.chat.transcript.is_empty()
            && m.history_messages.is_empty()
            && m.chat.live_messages.is_empty()
            && m.recovery.is_empty()
            && world.get::<Label>(ui.composer).unwrap().text.is_empty();
        world.insert_resource(if public {
            ObservationContent::Public
        } else {
            ObservationContent::Deny
        });
    }
}
