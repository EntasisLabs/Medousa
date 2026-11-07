mod backend;
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
}
#[derive(Clone, Copy)]
struct Ui {
    content: Entity,
    view: Entity,
    empty: Entity,
    composer: Entity,
    status: Entity,
    model: Entity,
    center: Entity,
    send: Entity,
    cancel: Entity,
    recover: Entity,
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
        ] {
            app.world_mut()
                .insert_resource(Viewport::new(width, height));
            app.update();
            if transcript {
                let model = app.world_mut().resource_mut::<Model>().unwrap();
                model.chat.transcript = "### You\n\nCan we make this a native chat?\n\n### Medousa\n\nYes — this is **rndr**, not a webview.\n\n- Real session history\n- Streaming chronological text\n- `Shift+Enter` for a new line\n\nThis is an offline visual fixture; no inference was sent.".into();
            }
            app.update();
            app.update();
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
    });
    app.add_plugin(rndr_app::FormNavigation);
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(SubmitBoundary::default());
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
    let header = row(world, root, Size::Px(34.));
    label(world, header, "Today", false);
    let reload = button(world, header, "reload", "Reload history");
    world.get_mut::<Layout>(reload).unwrap().width = Size::Px(150.);
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
    let view = markdown_view(world, "");
    child(world, content, view);
    world.insert(view, Hidden);
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
        gap: 10.,
        ..Layout::column()
    },));
    child(world, centered, center);
    let right = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, centered, right);
    let stickies = muted(world, center, "Sticky notes · unavailable");
    world.get_mut::<TextStyle>(stickies).unwrap().align = TextAlign::End;
    let box_ = world.spawn((
        Layout {
            padding: 14.,
            gap: 8.,
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
            height: Size::Px(72.),
            ..Layout::column()
        },
        Label::new(""),
        Prompt::new().max_lines(6),
        TextField,
        Focusable,
    ));
    world.insert(composer, Placeholder::new("Ask anything…"));
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
    let controls = row(world, box_, Size::Hug);
    let model = button(world, controls, "model", "Select model");
    world.get_mut::<Layout>(model).unwrap().width = Size::Fill;
    world.insert(model, Color::rgba(0., 0., 0., 0.));
    world.get_mut::<TextStyle>(model).unwrap().size = 13.;
    let send = button(world, controls, "send", "↑");
    world.get_mut::<Layout>(send).unwrap().width = Size::Px(44.);
    world.insert(
        send,
        Semantics {
            name: Some("Send message · may incur inference charges".into()),
            ..Default::default()
        },
    );
    world.insert(send, Color::rgb(0.85, 0.84, 0.82));
    world.get_mut::<Label>(send).unwrap().color = Some(theme.canvas);
    world.insert(send, Radius(22.));
    let cancel = button(world, controls, "cancel", "Stop");
    world.get_mut::<Layout>(cancel).unwrap().width = Size::Px(70.);
    muted(
        world,
        center,
        "+ Attachments / Mic — unavailable · Enter sends · Shift+Enter newline",
    );
    muted(
        world,
        center,
        "Medousa / Assistant / Behavior — defaults; editing not ported",
    );
    let bottom_space = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(50.),
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
        content,
        view,
        empty,
        composer,
        status,
        model,
        center,
        send,
        cancel,
        recover,
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
            && !matches!(reply.update, Update::Models(_) | Update::Unavailable(_))
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
                m.generation += 1;
                dispatch(world, command);
            }
            "model" if !busy => {
                let m = world.resource_mut::<Model>().unwrap();
                if !m.models.is_empty() {
                    m.selection = Some(m.selection.map_or(0, |i| (i + 1) % m.models.len()));
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
                        "Select a model first (button cycles catalog); sending may incur charges"
                            .into();
                    continue;
                };
                let (provider, model) = m.models[index].clone();
                match m.chat.begin(&prompt) {
                    Ok(()) => {
                        m.generation += 1;
                        m.pending_recovery = Some(m.recovery.len());
                        m.recovery
                            .push((m.chat.session.clone().unwrap(), prompt.clone()));
                        let request = turn_request(
                            m.chat.session.as_deref().unwrap(),
                            &prompt,
                            &provider,
                            &model,
                        );
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
    let mut text = m.chat.transcript.clone();
    if !m.chat.busy {
        for (session, prompt) in &m.recovery {
            text.push_str(&format!("\n\n### Recoverable prompt · {session}\n\nAdmission was not confirmed. Memory only; not automatically resubmitted. Copy into the composer only after checking history.\n\n{prompt}"));
        }
    }
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
        .map(|i| format!("{} / {} ▾", m.models[i].0, m.models[i].1))
        .unwrap_or_else(|| "Select model · click to cycle".into());
    let busy = m.chat.busy;
    let uncertain = m.chat.uncertain;
    let connected = m.commands.is_some();
    if text.is_empty() {
        world.insert(ui.view, Hidden);
        world.remove::<Hidden>(ui.empty);
    } else {
        world.remove::<Hidden>(ui.view);
        world.insert(ui.empty, Hidden);
    }
    if world.get::<MarkdownView>(ui.view).unwrap().source() != text {
        world
            .get_mut::<MarkdownView>(ui.view)
            .unwrap()
            .set_markdown(text);
    }
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
    world.get_mut::<Layout>(ui.center).unwrap().width = Size::Px((width - 48.).clamp(250., 670.));
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
            "model" => can_select,
            "new" | "recent" => can_navigate,
            "reload" => can_navigate && has_session,
            "cancel" => can_cancel,
            "recover" => can_recover,
            _ => true,
        };
    }
    let _ = ui.content;
    #[cfg(feature = "observe")]
    if world.has_resource::<FixtureObservation>() {
        use rndr_render::observe::ObservationContent;
        // Explicit fixture opt-in must not expose text someone types into the
        // native window while a read-only observer is attached.
        let m = world.resource::<Model>().unwrap();
        let public = m.commands.is_none()
            && m.chat.transcript.is_empty()
            && m.recovery.is_empty()
            && world.get::<Label>(ui.composer).unwrap().text.is_empty();
        world.insert_resource(if public {
            ObservationContent::Public
        } else {
            ObservationContent::Deny
        });
    }
}
