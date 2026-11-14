//! App-owned orchestration. RNDR owns controls/editing; app-core owns chat state.
use crate::{
    approvals, attachments, auxiliary,
    backend::{Command, Reply, Request, Update},
    chat::Chat,
    context, provider_timeline, runtimes, timeline,
};
use medousa_app_core::TurnFact;
use rndr_core::{
    editing::{DocumentId, SourceDocuments},
    *,
};
use rndr_ui::{
    controls::{self, ButtonProps, HostIntent, NavigationMeta},
    editing::{EditorSurface, EditorSurfaceHandle, EditorSurfaceProps},
    selection::{
        OptionId, SelectOption, SelectionChange, SelectionProps, SelectionValue, SingleSelect,
    },
    *,
};
use std::collections::HashMap;

const BEHAVIOR: u64 = 33;
const DETAILS: u64 = 30;
const MODEL_SEARCH: u64 = 31;
const SIDEBAR: u64 = 32;
const NEW: u64 = 1;
const SEARCH: u64 = 2;
const RELOAD: u64 = 3;
const SEND: u64 = 4;
const STOP: u64 = 5;
const SESSION: u64 = 1000;
const MODEL_PREV: u64 = 6;
const MODEL_NEXT: u64 = 7;
const MORE: u64 = 8;
const RENAME: u64 = 20;
const FORK: u64 = 21;
const PIN: u64 = 22;
const UNPIN: u64 = 26;
const DELETE: u64 = 23;
const DELETE_CONFIRM: u64 = 24;
const DELETE_CANCEL: u64 = 25;

pub struct Model {
    pub chat: Chat,
    pub commands: Option<tokio::sync::mpsc::UnboundedSender<Request>>,
    generation: u64,
    loading: bool,
    models: Vec<(String, String)>,
    selected: Option<usize>,
    sessions: Vec<(String, String)>,
    modes: Vec<(String, String, bool)>,
    mode: Option<String>,
    depth: usize,
    search_query: String,
    session_cursor: Option<String>,
    delete_target: Option<String>,
    pending_media: Vec<medousa_types::MediaRef>,
    model_filter: String,
    model_page: usize,
    drafts: HashMap<String, DocumentId>,
    external_drafts: HashMap<String, (DocumentId, rndr_core::editing::Revision)>,
    approval_refresh: std::time::Instant,
    selectors_key: String,
    sidebar_key: String,
    details_open: bool,
    model_search_open: bool,
    behavior_open: bool,
    sidebar_open: bool,
    document: DocumentId,
    pending_draft: Option<(DocumentId, rndr_core::editing::Revision)>,
}
#[derive(Clone, Copy)]
struct Ui {
    sidebar: Entity,
    details: Entity,
    filter_row: Entity,
    compose: Entity,
    sessions: Entity,
    search: Entity,
    status: Entity,
    rename: Entity,
    session_actions: Entity,
    header: Entity,
    timeline: Entity,
    composer_parent: Entity,
    composer: EditorSurfaceHandle,
    model: Entity,
    mode: Entity,
    depth: Entity,
    approvals: Entity,
    attachments: Entity,
    runtime: Entity,
    context: Entity,
    controls: Entity,
    footer: Entity,
    behavior: Entity,
    model_caption: Entity,
    model_filter: Entity,
    model_pages: Entity,
}

pub fn app() -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.world_mut().resource_mut::<Keymap>().unwrap().bind(
        KeyBinding::new("chat.send", [Keystroke::named(NamedKey::Enter).primary()])
            .context("Chat composer"),
    );
    let theme = UiTheme {
        appearance: Appearance::Dark,
        density: Density::Compact,
        ..Default::default()
    };
    rndr_ui::apply_theme_bridge(app.world_mut(), theme);
    let mut docs = SourceDocuments::default();
    let document = docs.open("");
    app.world_mut().insert_resource(docs);
    app.world_mut().insert_resource(Model {
        chat: Chat::connecting(),
        commands: None,
        generation: 0,
        loading: false,
        models: vec![],
        selected: None,
        sessions: vec![],
        modes: vec![],
        mode: None,
        depth: 1,
        search_query: String::new(),
        session_cursor: None,
        delete_target: None,
        pending_media: vec![],
        model_filter: String::new(),
        model_page: 0,
        drafts: HashMap::new(),
        external_drafts: HashMap::new(),
        approval_refresh: std::time::Instant::now(),
        selectors_key: String::new(),
        sidebar_key: String::new(),
        details_open: false,
        model_search_open: false,
        behavior_open: false,
        sidebar_open: true,
        document,
        pending_draft: None,
    });
    app.world_mut().insert_resource(WindowConfig {
        title: "Medousa".into(),
        size: LogicalSize::new(1200., 850.),
        min_size: Some(LogicalSize::new(760., 540.)),
        ..Default::default()
    });
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, update);
    app
}
fn container(world: &mut World, parent: Option<Entity>, layout: Layout) -> Entity {
    let e = world.spawn((layout,));
    if let Some(p) = parent {
        set_parent(world, e, p);
    }
    e
}
fn button(
    world: &mut World,
    parent: Entity,
    id: u64,
    title: &str,
    enabled: bool,
) -> controls::ControlHandle {
    let mut props = ButtonProps::new(title, HostIntent::Command(id));
    props.enabled = enabled;
    props.name = match id {
        SEND => Some("Send message".into()),
        STOP => Some("Stop response".into()),
        _ => None,
    };
    props.variant = if id == SEND || id == STOP {
        controls::ButtonVariant::Primary
    } else {
        controls::ButtonVariant::Quiet
    };
    let h = controls::Button::mount(world, parent, UiId(id), props).expect("stable control");
    world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
    if world
        .get::<Layout>(parent)
        .is_some_and(|l| l.axis == Axis::Row)
    {
        world.get_mut::<Layout>(h.root).unwrap().width = Size::Px(match id {
            SEND | STOP => 54.,
            DETAILS | SIDEBAR => 36.,
            MODEL_PREV | MODEL_NEXT => 70.,
            MODEL_SEARCH => 150.,
            _ => 90.,
        });
    }
    h
}
fn plane(world: &mut World, entity: Entity, color: Color) {
    world.insert(
        entity,
        UiStyle {
            background: Some(UiColor::Explicit(color)),
            ..Default::default()
        },
    );
}
fn setup(world: &mut World) {
    let root = container(
        world,
        None,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
    );
    let sidebar = Surface::Navigation.mount(world, root, UiId(1)).unwrap();
    *world.get_mut::<Layout>(sidebar.root).unwrap() = Layout {
        width: Size::Px(230.),
        height: Size::Fill,
        ..Layout::column()
    };
    *world.get_mut::<Layout>(sidebar.content).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        padding: 12.,
        gap: 8.,
        ..Layout::column()
    };
    plane(world, sidebar.root, Color::rgb(0.065, 0.065, 0.075));
    text(world, sidebar.content, "Medousa", Typography::Heading).unwrap();
    button(world, sidebar.content, NEW, "＋  New chat", true);
    let search = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(32.),
            padding: 6.,
            ..Layout::column()
        },
        Label::new(""),
        TextField,
        Focusable,
        Placeholder::new("Search conversations…"),
    ));
    set_parent(world, search, sidebar.content);
    world.insert(
        search,
        Semantics {
            name: Some("Search conversations".into()),
            ..Default::default()
        },
    );
    let sessions = container(
        world,
        Some(sidebar.content),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 2.,
            ..Layout::column()
        },
    );
    world.insert(sessions, Scroll::default());
    let content = Surface::Content.mount(world, root, UiId(2)).unwrap();
    *world.get_mut::<Layout>(content.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    };
    *world.get_mut::<Layout>(content.content).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 4.,
        ..Layout::column()
    };
    plane(world, content.root, Color::rgb(0.035, 0.035, 0.04));
    let header_row = container(
        world,
        Some(content.content),
        Layout {
            width: Size::Fill,
            height: Size::Px(42.),
            padding: 4.,
            gap: 8.,
            ..Layout::row()
        },
    );
    plane(world, header_row, Color::rgb(0.065, 0.065, 0.075));
    button(world, header_row, SIDEBAR, "☰", true);
    let header = text(world, header_row, "Today", Typography::Label).unwrap();
    world.get_mut::<Layout>(header).unwrap().width = Size::Fill;
    world.get_mut::<Label>(header).unwrap().overflow = TextOverflow::Ellipsis;
    button(world, header_row, DETAILS, "⋯", true);
    let details = container(
        world,
        Some(content.content),
        Layout {
            width: Size::Fill,
            height: Size::Px(220.),
            padding: 8.,
            gap: 6.,
            ..Layout::column()
        },
    );
    world.insert(details, Scroll::default());
    world.insert(details, Hidden);
    let session_actions = container(
        world,
        Some(details),
        Layout {
            width: Size::Fill,
            gap: 4.,
            ..Layout::row()
        },
    );
    let rename = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(30.),
            ..Layout::column()
        },
        Label::new(""),
        TextField,
        Focusable,
        Placeholder::new("Conversation title"),
    ));
    set_parent(world, rename, session_actions);
    for (id, label) in [
        (RENAME, "Rename"),
        (FORK, "Fork"),
        (PIN, "Pin"),
        (UNPIN, "Unpin"),
        (DELETE, "Delete"),
        (DELETE_CONFIRM, "Confirm deletion"),
        (DELETE_CANCEL, "Keep chat"),
    ] {
        button(world, session_actions, id, label, false);
    }
    button(world, details, RELOAD, "Reload conversation", true);
    let context = context::setup(world, details);
    let runtime = runtimes::setup(world, details);
    let timeline = timeline::setup(world, content.content);
    let approvals = approvals::setup(world, content.content);
    let composer_row = container(
        world,
        Some(content.content),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            padding: 12.,
            ..Layout::row()
        },
    );
    container(
        world,
        Some(composer_row),
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
    );
    let composer_shell = container(
        world,
        Some(composer_row),
        Layout {
            width: Size::Px(700.),
            height: Size::Hug,
            gap: 4.,
            ..Layout::column()
        },
    );
    let compose = Surface::Raised
        .mount(world, composer_shell, UiId(3))
        .unwrap();
    *world.get_mut::<Layout>(compose.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Hug,
        ..Layout::column()
    };
    *world.get_mut::<Layout>(compose.content).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Hug,
        padding: 8.,
        gap: 0.,
        ..Layout::column()
    };
    world.insert(
        compose.root,
        UiStyle {
            background: Some(UiColor::Explicit(composer_color())),
            padding: Some(Space::Zero),
            gap: Some(Space::Zero),
            ..Default::default()
        },
    );
    world.insert(compose.root, Radius(24.));
    world.insert(
        compose.root,
        Border {
            color: Color::rgb(0.22, 0.22, 0.25),
            width: 1.,
        },
    );
    container(
        world,
        Some(composer_row),
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
    );
    let attachment_tray = container(
        world,
        Some(compose.content),
        Layout {
            width: Size::Fill,
            height: Size::Px(0.),
            gap: 2.,
            ..Layout::column()
        },
    );
    world.insert(attachment_tray, Scroll::default());
    world.insert(attachment_tray, Hidden);
    let composer_parent = container(
        world,
        Some(compose.content),
        Layout {
            width: Size::Fill,
            height: Size::Px(44.),
            padding: 4.,
            ..Layout::column()
        },
    );
    world.insert(composer_parent, Context::new("Chat composer"));
    let doc = world.resource::<Model>().unwrap().document;
    let composer = mount_composer(world, composer_parent, doc);
    let controls = container(
        world,
        Some(compose.content),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            ..Layout::row()
        },
    );
    let attachments = attachments::setup(world, controls, attachment_tray);
    world.get_mut::<Layout>(attachments).unwrap().width = Size::Px(40.);
    let model_stack = container(
        world,
        Some(controls),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 0.,
            ..Layout::column()
        },
    );
    let model = SingleSelect::mount(
        world,
        model_stack,
        UiId(10),
        SelectionProps::new("Model"),
        &[],
        None,
    )
    .unwrap()
    .root;
    let model_caption = text(world, model_stack, "Choose a model", Typography::Secondary).unwrap();
    world.get_mut::<Layout>(model_caption).unwrap().padding = 0.;
    let footer = container(
        world,
        Some(composer_shell),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            ..Layout::row()
        },
    );
    let runtime_trigger = button(world, footer, DETAILS, "Medousa  ⌄", true);
    world.get_mut::<Layout>(runtime_trigger.root).unwrap().width = Size::Px(118.);
    let mode = SingleSelect::mount(
        world,
        footer,
        UiId(11),
        SelectionProps::new("Mode"),
        &[],
        None,
    )
    .unwrap()
    .root;
    container(
        world,
        Some(footer),
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
    );
    button(world, footer, BEHAVIOR, "Behavior  ⌄", true);
    let behavior = container(
        world,
        Some(composer_shell),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    world.insert(behavior, Hidden);
    let depth = SingleSelect::mount(
        world,
        behavior,
        UiId(12),
        SelectionProps::new("Depth"),
        &depth_options(),
        Some(OptionId(1)),
    )
    .unwrap()
    .root;
    button(world, behavior, MODEL_SEARCH, "Search models", true);
    button(world, controls, STOP, "Stop", false);
    button(world, controls, SEND, "↑", false);
    let filter_row = container(
        world,
        Some(composer_shell),
        Layout {
            width: Size::Fill,
            height: Size::Px(32.),
            gap: 4.,
            ..Layout::row()
        },
    );
    world.insert(filter_row, Hidden);
    let model_filter = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Label::new(""),
        TextField,
        Focusable,
        Placeholder::new("Search models…"),
    ));
    set_parent(world, model_filter, filter_row);
    button(world, filter_row, MODEL_PREV, "Previous", true);
    let model_pages = text(world, filter_row, "", Typography::Secondary).unwrap();
    world.get_mut::<Layout>(model_pages).unwrap().width = Size::Px(50.);
    button(world, filter_row, MODEL_NEXT, "Next", true);
    let status = text(
        world,
        content.content,
        "Connecting to local daemon…",
        Typography::Secondary,
    )
    .unwrap();
    world.get_mut::<Layout>(status).unwrap().height = Size::Px(22.);
    world.get_mut::<Layout>(status).unwrap().padding = 3.;
    world.insert_resource(Ui {
        sidebar: sidebar.root,
        details,
        filter_row,
        compose: composer_shell,
        sessions,
        search,
        status,
        rename,
        session_actions,
        header,
        timeline,
        composer_parent,
        composer,
        model,
        mode,
        depth,
        approvals,
        attachments,
        runtime,
        context,
        controls,
        footer,
        behavior,
        model_caption,
        model_filter,
        model_pages,
    });
}
fn mount_composer(world: &mut World, parent: Entity, document: DocumentId) -> EditorSurfaceHandle {
    let mut props = EditorSurfaceProps::new(document, "Message Medousa");
    props.gutter = false;
    props.scrollbar = false;
    props.description = "Write a message, then Send. Enter inserts a new line.".into();
    let h = EditorSurface::mount(world, parent, UiId(1), props).expect("live source document");
    world.insert(
        h.editor,
        UiStyle {
            background: Some(UiColor::Explicit(Color::rgb(0.085, 0.085, 0.105))),
            typography: Some(Typography::Label),
            ..Default::default()
        },
    );
    // Core editor paint otherwise supplies its own gray input background.
    world.insert(h.editor, ResolvedColor);
    world.insert(h.editor, composer_color());
    world.insert(h.editor, Placeholder::new("Message Medousa…"));
    h
}
fn composer_color() -> Color {
    Color::rgb(0.085, 0.085, 0.105)
}

fn model_display_name(name: &str) -> String {
    // Keep unknown/provider-specific model names intact.
    match name {
        "gpt-6.1-sol" => "GPT-6.1 Sol".into(),
        _ => name.into(),
    }
}
fn provider_caption(provider: &str) -> String {
    match provider {
        "openai-codex" => "Using ChatGPT".into(),
        "openrouter" => "Using OpenRouter".into(),
        _ => format!("Using {provider}"),
    }
}
fn quiet_selector(
    world: &mut World,
    root: Entity,
    id: u64,
    title: &str,
    enabled: bool,
    width: Size,
) {
    // Retain RNDR's selector event routing/popup and accessible name. Only its
    // trigger presentation changes; do not traverse/style popup option rows.
    let name = world.get::<Semantics>(root).and_then(|s| s.name.clone());
    let parent = world.get::<Parent>(root).unwrap().entity;
    let mut props = ButtonProps::new(title, HostIntent::Command(id));
    props.variant = controls::ButtonVariant::Quiet;
    props.enabled = enabled;
    props.name = name;
    let handle = controls::Button::mount(world, parent, UiId(id), props).unwrap();
    world.get_mut::<Layout>(root).unwrap().width = width;
    world.get_mut::<Label>(handle.label).unwrap().overflow = TextOverflow::Ellipsis;
    // Selectors remain visually quiet even while disabled during a turn.
    // Keep the native outer focus ring and the popup untouched.
    let face = world.get::<Parent>(handle.label).unwrap().entity;
    let border = world.get::<Parent>(face).unwrap().entity;
    let color = if id == 10 {
        composer_color()
    } else {
        Color::rgb(0.035, 0.035, 0.04)
    };
    plane(world, face, color);
    plane(world, border, color);
}
fn round_send(world: &mut World, handle: controls::ControlHandle, enabled: bool) {
    let mut nodes = vec![handle.root];
    while let Some(node) = nodes.pop() {
        // Shape the existing RNDR control primitives, preserving its focus,
        // hover, disabled state and activation semantics.
        let radius = if node == handle.root { 19. } else { 15. };
        world.insert(node, Radius(radius));
        if node != handle.label {
            world.get_mut::<Layout>(node).unwrap().height = Size::Fill;
        }
        nodes.extend(children_of(world, node));
    }
    *world.get_mut::<Layout>(handle.root).unwrap() = Layout {
        width: Size::Px(38.),
        height: Size::Px(38.),
        padding: 2.,
        ..Layout::column()
    };
    let face = world.get::<Parent>(handle.label).unwrap().entity;
    let border = world.get::<Parent>(face).unwrap().entity;
    let color = if enabled {
        Color::rgb(0.94, 0.93, 0.90)
    } else {
        Color::rgb(0.25, 0.25, 0.28)
    };
    plane(world, face, color);
    plane(world, border, color);
    // Fill the circular face with centered label content, rather than using
    // a short Hug-height face (which produces an over-rounded diamond).
    world.get_mut::<Layout>(handle.label).unwrap().height = Size::Fill;
    if let Some(metrics) = world.get_mut::<LabelBox>(handle.label) {
        metrics.inset_x = 7.;
        metrics.inset_y = 3.;
    }
    world.insert(
        handle.label,
        UiStyle {
            foreground: Some(UiColor::Explicit(if enabled {
                Color::rgb(0.07, 0.07, 0.08)
            } else {
                Color::rgb(0.55, 0.55, 0.59)
            })),
            ..Default::default()
        },
    );
}
fn depth_options() -> Vec<SelectOption> {
    ["Concise", "Standard", "Deep"]
        .iter()
        .enumerate()
        .map(|(i, s)| SelectOption::new(i as u64, *s))
        .collect()
}
fn dispatch(world: &mut World, command: Command) {
    let m = world.resource_mut::<Model>().unwrap();
    if m.commands.as_ref().is_none_or(|tx| {
        tx.send(Request {
            generation: m.generation,
            command,
        })
        .is_err()
    }) {
        m.loading = false;
        m.chat.status =
            "Local daemon connection unavailable. Start the daemon and restart Medousa.".into();
    }
}
fn switch_document(world: &mut World, session: &str) -> Result<(), String> {
    let ui = *world.resource::<Ui>().unwrap();
    let old = world.resource::<Model>().unwrap().document;
    if world
        .resource::<SourceDocuments>()
        .unwrap()
        .get(old)
        .unwrap()
        .is_composing()
    {
        return Err("Finish composing before switching conversations.".into());
    }
    let known = world
        .resource::<Model>()
        .unwrap()
        .drafts
        .get(session)
        .copied();
    let doc = known.unwrap_or_else(|| world.resource_mut::<SourceDocuments>().unwrap().open(""));
    if doc == old {
        return Ok(());
    }
    ui.composer
        .destroy(world)
        .map_err(|e| format!("Composer unavailable: {e:?}"))?;
    let composer = mount_composer(world, ui.composer_parent, doc);
    let m = world.resource_mut::<Model>().unwrap();
    m.document = doc;
    m.drafts.insert(session.into(), doc);
    world.resource_mut::<Ui>().unwrap().composer = composer;
    Ok(())
}
fn update(world: &mut World) {
    auxiliary::tick(world);
    for reply in events::<Reply>(world).to_vec() {
        apply_reply(world, reply);
    }
    let ui = *world.resource::<Ui>().unwrap();
    if world
        .resource::<KeymapFired>()
        .is_some_and(|k| k.actions.iter().any(|a| a == "chat.send"))
    {
        action(world, SEND);
    }
    for event in events::<SelectionChange>(world).to_vec() {
        let SelectionValue::Selected(ids) = event.value else {
            continue;
        };
        let Some(id) = ids.first().map(|i| i.0 as usize) else {
            continue;
        };
        let m = world.resource_mut::<Model>().unwrap();
        if m.chat.busy() || m.loading {
            continue;
        }
        if event.control == ui.model && id < m.models.len() {
            m.selected = Some(id);
        }
        if event.control == ui.depth && id < 3 {
            m.depth = id;
        }
        if event.control == ui.mode && id < m.modes.len() && m.modes[id].2 {
            if let (Some(session), Ok(mode)) = (
                m.chat.session().map(str::to_owned),
                serde_json::from_value(serde_json::Value::String(m.modes[id].0.clone())),
            ) {
                m.loading = true;
                dispatch(world, Command::SetMode(session, mode));
            }
        }
    }
    for intent in take_action_intents(world) {
        if approvals::action(world, &intent)
            || attachments::action(world, &intent)
            || runtimes::action(world, &intent)
        {
            continue;
        }
        if timeline::action(world, &intent) {
            continue;
        }
        if let Some(host) = controls::host_intent(world, &intent) {
            if context::action(world, &host) {
                continue;
            }
            if let HostIntent::Command(id) = host {
                action(world, id);
            }
        }
    }
    if events::<Submitted>(world)
        .iter()
        .any(|s| s.entity == ui.search)
    {
        action(world, SEARCH);
    }
    if let Some(error) = world
        .resource_mut::<auxiliary::Auxiliary>()
        .and_then(|a| a.error.take())
    {
        world.resource_mut::<Model>().unwrap().chat.status = error;
    }
    sync(world);
    auxiliary::dispatch(world);
}
fn action(world: &mut World, id: u64) {
    let ui = *world.resource::<Ui>().unwrap();
    match id {
        BEHAVIOR => {
            let m = world.resource_mut::<Model>().unwrap();
            m.behavior_open = !m.behavior_open;
        }
        DETAILS => {
            let m = world.resource_mut::<Model>().unwrap();
            m.details_open = !m.details_open;
        }
        MODEL_SEARCH => {
            let m = world.resource_mut::<Model>().unwrap();
            m.model_search_open = !m.model_search_open;
        }
        SIDEBAR => {
            let m = world.resource_mut::<Model>().unwrap();
            m.sidebar_open = !m.sidebar_open;
        }
        MORE => {
            let m = world.resource::<Model>().unwrap();
            if let Some(cursor) = m.session_cursor.clone() {
                dispatch(
                    world,
                    Command::More {
                        query: m.search_query.clone(),
                        cursor,
                    },
                );
            }
        }
        RENAME | FORK | PIN | UNPIN | DELETE | DELETE_CONFIRM | DELETE_CANCEL => {
            let m = world.resource::<Model>().unwrap();
            if m.loading {
                return;
            }
            let Some(session) = m.chat.session().map(str::to_owned) else {
                return;
            };
            let command = match id {
                RENAME => {
                    let title = world
                        .get::<Label>(ui.rename)
                        .unwrap()
                        .text
                        .trim()
                        .to_owned();
                    if title.is_empty() {
                        return;
                    }
                    Command::Rename { session, title }
                }
                FORK => Command::Fork(session),
                PIN | UNPIN => Command::Pin {
                    session,
                    pinned: id == PIN,
                },
                DELETE => {
                    world.resource_mut::<Model>().unwrap().delete_target = Some(session);
                    return;
                }
                DELETE_CANCEL => {
                    world.resource_mut::<Model>().unwrap().delete_target = None;
                    return;
                }
                _ => {
                    let Some(target) = world.resource_mut::<Model>().unwrap().delete_target.take()
                    else {
                        return;
                    };
                    if target != session {
                        return;
                    }
                    Command::Delete(target)
                }
            };
            dispatch(world, command);
        }
        MODEL_PREV | MODEL_NEXT => {
            let m = world.resource_mut::<Model>().unwrap();
            if id == MODEL_NEXT {
                m.model_page += 1;
            } else {
                m.model_page = m.model_page.saturating_sub(1);
            }
        }
        SEARCH => {
            let query = world.get::<Label>(ui.search).unwrap().text.clone();
            world.resource_mut::<Model>().unwrap().search_query = query.trim().to_owned();
            dispatch(world, Command::List(query));
        }
        NEW | RELOAD | SESSION.. => {
            let m = world.resource::<Model>().unwrap();
            if m.loading {
                return;
            }
            let doc = world
                .resource::<SourceDocuments>()
                .unwrap()
                .get(m.document)
                .unwrap();
            if doc.is_composing() {
                world.resource_mut::<Model>().unwrap().chat.status =
                    "Finish composing before switching conversations.".into();
                return;
            }
            let command = match id {
                NEW => Command::New,
                RELOAD => match m.chat.session() {
                    Some(s) => Command::Open(s.into()),
                    None => Command::List(String::new()),
                },
                _ => match m.sessions.get((id - SESSION) as usize) {
                    Some((s, _)) => Command::Open(s.clone()),
                    None => return,
                },
            };
            let m = world.resource_mut::<Model>().unwrap();
            m.generation += 1;
            m.loading = true;
            m.chat.status = "Loading conversation…".into();
            m.mode = None;
            m.pending_draft = None;
            dispatch(world, command);
        }
        SEND => {
            let session = world
                .resource::<Model>()
                .unwrap()
                .chat
                .session()
                .map(str::to_owned);
            if let Some(session) = session {
                if !matches!(runtimes::active(world), runtimes::ActiveRuntime::Native) {
                    let id = world.resource::<Model>().unwrap().document;
                    let Some(doc) = world.resource::<SourceDocuments>().unwrap().get(id) else {
                        return;
                    };
                    if doc.is_composing() || doc.text().trim().is_empty() {
                        return;
                    }
                    let text = doc.text().to_owned();
                    let revision = doc.revision();
                    let extra =
                        attachments::media_refs(world, &session).map(|refs| !refs.is_empty());
                    let result =
                        extra.and_then(|extra| runtimes::send(world, &session, text, extra));
                    match result {
                        Ok(true) => {
                            world
                                .resource_mut::<Model>()
                                .unwrap()
                                .external_drafts
                                .insert(session, (id, revision));
                        }
                        Ok(false) => {}
                        Err(error) => world.resource_mut::<Model>().unwrap().chat.status = error,
                    }
                    return;
                }
            }
            let m = world.resource::<Model>().unwrap();
            if m.loading || m.chat.busy() || m.chat.uncertain {
                return;
            }
            let Some(session) = m.chat.session() else {
                world.resource_mut::<Model>().unwrap().chat.status =
                    "Create or open a conversation before sending.".into();
                return;
            };
            let Some((provider, model)) = m.selected.and_then(|i| m.models.get(i)) else {
                return;
            };
            let doc = world
                .resource::<SourceDocuments>()
                .unwrap()
                .get(m.document)
                .unwrap();
            if doc.is_composing() {
                return;
            }
            let pending = (m.document, doc.revision());
            let mut request = crate::chat::turn_request(session, doc.text(), provider, model);
            let refs = match attachments::media_refs(world, session) {
                Ok(refs) => refs,
                Err(error) => {
                    world.resource_mut::<Model>().unwrap().chat.status = error;
                    return;
                }
            };
            if doc.text().trim().is_empty() && refs.is_empty() {
                return;
            }
            request.media_refs = refs.clone();
            request.response_depth_mode = ["concise", "standard", "deep"][m.depth].into();
            let m = world.resource_mut::<Model>().unwrap();
            m.loading = true;
            m.pending_draft = Some(pending);
            m.pending_media = refs;
            dispatch(world, Command::Send(Box::new(request)));
        }
        STOP => {
            if let Some(session) = world
                .resource::<Model>()
                .unwrap()
                .chat
                .session()
                .map(str::to_owned)
            {
                if !matches!(runtimes::active(world), runtimes::ActiveRuntime::Native) {
                    let _ = runtimes::cancel(world, &session);
                    return;
                }
            }

            if let Some(id) = world
                .resource::<Model>()
                .unwrap()
                .chat
                .session()
                .map(str::to_owned)
            {
                dispatch(world, Command::Cancel(id));
            }
        }
        _ => {}
    }
}
fn apply_reply(world: &mut World, reply: Reply) {
    let m = world.resource::<Model>().unwrap();
    if reply.generation != m.generation
        && !matches!(
            reply.update,
            Update::Models(_) | Update::Modes(_) | Update::Unavailable(_)
        )
    {
        return;
    }
    match reply.update {
        Update::SessionsPage {
            query,
            rows,
            cursor,
            append,
        } => {
            let m = world.resource_mut::<Model>().unwrap();
            if query != m.search_query {
                return;
            }
            if append {
                for row in rows {
                    if !m.sessions.iter().any(|x| x.0 == row.0) {
                        m.sessions.push(row);
                    }
                }
            } else {
                m.sessions = rows;
            }
            m.session_cursor = cursor;
        }
        Update::Renamed { session, title } => {
            for row in &mut world.resource_mut::<Model>().unwrap().sessions {
                if row.0 == session {
                    row.1 = title.clone();
                }
            }
        }
        Update::Deleted(session) => {
            let m = world.resource_mut::<Model>().unwrap();
            m.sessions.retain(|r| r.0 != session);
            if m.chat.session() == Some(&session) {
                m.chat = Chat::connecting();
                m.chat.status = "Conversation deleted".into();
            }
        }
        Update::Forked { source: _, session } => {
            let m = world.resource_mut::<Model>().unwrap();
            m.generation += 1;
            m.loading = true;
            dispatch(world, Command::Open(session));
        }
        Update::History(session, authority, turns, active) => {
            if let Err(e) = switch_document(world, &session) {
                world.resource_mut::<Model>().unwrap().chat.fail(e);
                return;
            }
            if let Some(aux) = world.resource::<auxiliary::Auxiliary>() {
                aux.send(auxiliary::Command::Approval(approvals::refresh(&session)));
            }
            runtimes::refresh(world, &session);
            if let Some(aux) = world.resource::<auxiliary::Auxiliary>() {
                aux.send(auxiliary::Command::Context(context::refresh(&session)));
            }
            let active = active.map(|t| TurnFact {
                turn_id: t.turn_id,
                mode: "interactive".into(),
                phase: "streaming".into(),
                message_id: None,
                workspace_card_id: None,
                reusable_stream: true,
                budget_request_id: None,
                requested_rounds: None,
            });
            let m = world.resource_mut::<Model>().unwrap();
            if let Err(e) = m.chat.hydrate(session, authority, turns, active) {
                m.chat.fail(e);
            }
            restore_history_model(m);
            m.loading = false;
        }
        Update::SettledHistory(session, authority, turns) => {
            let m = world.resource_mut::<Model>().unwrap();
            if let Err(e) = m.chat.hydrate(session, authority, turns, None) {
                m.chat.fail(e);
            }
            m.loading = false;
        }
        Update::Admitted(turn, prompt) => {
            let m = world.resource_mut::<Model>().unwrap();
            if let Err(e) = m.chat.admitted(turn, prompt) {
                m.chat.fail(e);
            }
            m.loading = false;
            let session = m.chat.session().map(str::to_owned);
            let media = std::mem::take(&mut m.pending_media);
            let pending = m.pending_draft.take();
            if let Some(session) = session {
                attachments::clear_admitted(world, &session, &media);
            }
            if let Some((doc, revision)) = pending {
                let source = world
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(doc)
                    .unwrap();
                // Never erase text typed during admission or an in-progress IME composition.
                if source.revision() == revision && !source.is_composing() {
                    let _ = source.reload(revision, "");
                }
            }
        }
        Update::Stream(envelope) => {
            if let medousa_types::TurnStreamEventV3::ContextUsage { report, .. } = &envelope.event {
                if let Some(session) = world
                    .resource::<Model>()
                    .unwrap()
                    .chat
                    .session()
                    .map(str::to_owned)
                {
                    context::usage(world, &session, report.clone());
                }
            }
            let terminal = envelope.event.is_terminal();
            let m = world.resource_mut::<Model>().unwrap();
            if let Err(e) = m.chat.apply(envelope) {
                m.chat.fail(e);
            }
            if terminal {
                m.chat.uncertain = true;
                m.chat.status = "Reconciling history…".into();
            }
        }
        Update::Models(models) => {
            let m = world.resource_mut::<Model>().unwrap();
            let previous = m.selected.and_then(|i| m.models.get(i)).cloned();
            m.selected = previous
                .as_ref()
                .and_then(|s| models.iter().position(|x| x == s));
            if m.selected.is_none() && !models.is_empty() {
                m.selected = Some(0);
            }
            m.models = models;
            restore_history_model(m);
            m.chat.status = "Connected".into();
        }
        Update::Sessions(sessions) => {
            world.resource_mut::<Model>().unwrap().sessions = sessions;
        }
        Update::Modes(modes) => {
            world.resource_mut::<Model>().unwrap().modes = modes;
        }
        Update::Mode(mode) => {
            let m = world.resource_mut::<Model>().unwrap();
            m.mode = Some(mode);
            m.loading = false;
        }
        Update::Notice(message) => {
            let m = world.resource_mut::<Model>().unwrap();
            m.chat.status = message;
            m.loading = false;
        }
        Update::Failure(message) => {
            let m = world.resource_mut::<Model>().unwrap();
            m.chat.fail(message);
            m.loading = false;
            m.pending_draft = None;
        }
        Update::Unavailable(message) => {
            let m = world.resource_mut::<Model>().unwrap();
            m.chat.status = message;
            m.loading = false;
        }
    }
}
// Resolve the last persisted response against the current catalog. Never
// silently display catalog entry zero when reopening a known conversation.
fn restore_history_model(m: &mut Model) {
    let Some(item) = m
        .chat
        .snapshot()
        .and_then(|s| s.items.iter().rev().find(|i| i.response_model.is_some()))
    else {
        return;
    };
    let Some(model) = item.response_model.as_ref() else {
        return;
    };
    let exact = m
        .models
        .iter()
        .position(|(p, n)| Some(p) == item.response_provider.as_ref() && n == model);
    let mut same_model = m.models.iter().enumerate().filter(|(_, (_, n))| n == model);
    let unique = same_model.next().and_then(|(i, _)| {
        if same_model.next().is_none() {
            Some(i)
        } else {
            None
        }
    });
    if let Some(i) = exact.or(unique) {
        m.selected = Some(i);
    }
}

fn sync(world: &mut World) {
    let ui = *world.resource::<Ui>().unwrap();
    // Borrow presentation data, temporarily moving only the core adapter. RNDR
    // World deliberately has no resource removal API. No parallel state owner.
    let (chat, loading, models, selected, sessions, modes, mode, depth, document, connected) = {
        let m = world.resource_mut::<Model>().unwrap();
        (
            std::mem::take(&mut m.chat),
            m.loading,
            m.models.clone(),
            m.selected,
            m.sessions.clone(),
            m.modes.clone(),
            m.mode.clone(),
            m.depth,
            m.document,
            m.commands.is_some(),
        )
    };
    struct Presentation {
        chat: Chat,
        loading: bool,
        models: Vec<(String, String)>,
        selected: Option<usize>,
        sessions: Vec<(String, String)>,
        modes: Vec<(String, String, bool)>,
        mode: Option<String>,
        depth: usize,
        document: DocumentId,
        connected: bool,
    }
    let m = Presentation {
        chat,
        loading,
        models,
        selected,
        sessions,
        modes,
        mode,
        depth,
        document,
        connected,
    };
    world.get_mut::<Label>(ui.status).unwrap().text = m.chat.status.clone();
    let title = m
        .chat
        .session()
        .and_then(|id| m.sessions.iter().find(|(s, _)| s == id))
        .map(|(_, title)| title.as_str())
        .unwrap_or("Today");
    approvals::sync(world, ui.approvals, m.chat.session(), m.chat.snapshot());
    runtimes::sync(world, ui.runtime, m.chat.session());
    context::sync(world, ui.context, m.chat.session());
    let refresh = world
        .resource::<Model>()
        .unwrap()
        .approval_refresh
        .elapsed()
        > std::time::Duration::from_secs(30);
    if refresh {
        world.resource_mut::<Model>().unwrap().approval_refresh = std::time::Instant::now();
        if let Some(session) = m.chat.session() {
            runtimes::refresh(world, session);
            if let Some(aux) = world.resource::<auxiliary::Auxiliary>() {
                aux.send(auxiliary::Command::Approval(approvals::refresh(session)));
                aux.send(auxiliary::Command::Context(context::refresh(session)));
            }
        }
    }

    attachments::sync(world, ui.attachments, m.chat.session());
    world.get_mut::<Label>(ui.header).unwrap().text = title.into();
    let external = !matches!(runtimes::active(world), runtimes::ActiveRuntime::Native);
    let available = !m.loading && !m.chat.busy() && !runtimes::busy(world);
    let query = world
        .get::<Label>(ui.model_filter)
        .unwrap()
        .text
        .to_lowercase();
    let state = world.resource_mut::<Model>().unwrap();
    if state.model_filter != query {
        state.model_filter = query.clone();
        state.model_page = 0;
    }
    let selector_key = format!(
        "{:?}{:?}{:?}{:?}{}{}{}",
        m.models, m.modes, m.selected, m.mode, m.depth, available, query
    );
    let selector_key = format!("{selector_key}{}", state.model_page);
    let selectors_changed = state.selectors_key != selector_key;
    state.selectors_key = selector_key;
    if selectors_changed {
        let state = world.resource_mut::<Model>().unwrap();
        let (models, pages, page) = model_options(&m.models, m.selected, &query, state.model_page);
        state.model_page = page;
        world.get_mut::<Label>(ui.model_pages).unwrap().text = format!("{} / {}", page + 1, pages);
        button(
            world,
            world.get::<Parent>(ui.model_pages).unwrap().entity,
            MODEL_PREV,
            "Previous",
            page > 0,
        );
        button(
            world,
            world.get::<Parent>(ui.model_pages).unwrap().entity,
            MODEL_NEXT,
            "Next",
            page + 1 < pages,
        );
        let mut props = SelectionProps::new("Model");
        props.enabled = available;
        SingleSelect::mount(
            world,
            world.get::<Parent>(ui.model).unwrap().entity,
            UiId(10),
            props,
            &models,
            m.selected.map(|i| OptionId(i as u64)),
        )
        .unwrap();
        let modes = m
            .modes
            .iter()
            .enumerate()
            .map(|(i, (_, n, enabled))| {
                let mut o = SelectOption::new(i as u64, n);
                o.enabled = *enabled;
                o
            })
            .collect::<Vec<_>>();
        let mut props = SelectionProps::new("Mode");
        props.enabled = available && m.chat.session().is_some();
        let selected = m
            .mode
            .as_ref()
            .and_then(|mode| m.modes.iter().position(|(id, _, _)| id == mode))
            .map(|i| OptionId(i as u64));
        SingleSelect::mount(world, ui.footer, UiId(11), props, &modes, selected).unwrap();
        let mut props = SelectionProps::new("Depth");
        props.enabled = available;
        SingleSelect::mount(
            world,
            ui.behavior,
            UiId(12),
            props,
            &depth_options(),
            Some(OptionId(m.depth as u64)),
        )
        .unwrap();
        for (entity, width) in [
            (ui.model, Size::Fill),
            (ui.mode, Size::Px(112.)),
            (ui.depth, Size::Px(110.)),
        ] {
            world.get_mut::<Layout>(entity).unwrap().width = width;
            let mut pending = children_of(world, entity);
            while let Some(child) = pending.pop() {
                if let Some(label) = world.get_mut::<Label>(child) {
                    label.overflow = TextOverflow::Ellipsis;
                }
                pending.extend(children_of(world, child));
            }
        }
    }
    let doc = world
        .resource::<SourceDocuments>()
        .unwrap()
        .get(m.document)
        .unwrap();
    // Grow with explicit draft lines, then let the native editor scroll. Do
    // not remount the source document or reset its selection/undo on typing.
    let editor_height = (doc.text().lines().count().max(1).min(6) as f32 * 22. + 22.).max(44.);
    world.get_mut::<Layout>(ui.composer_parent).unwrap().height = Size::Px(editor_height);
    let doc = world
        .resource::<SourceDocuments>()
        .unwrap()
        .get(m.document)
        .unwrap();
    let send_enabled = available
        && !m.chat.uncertain
        && m.connected
        && m.chat.session().is_some()
        && (external || m.selected.is_some())
        && (!doc.text().trim().is_empty()
            || m.chat
                .session()
                .is_some_and(|s| attachments::media_refs(world, s).is_ok_and(|r| !r.is_empty())))
        && !doc.is_composing();
    let send = button(world, ui.controls, SEND, "↑", send_enabled);
    round_send(world, send, send_enabled);
    let model_title = m
        .selected
        .and_then(|i| m.models.get(i))
        .map(|(_, name)| model_display_name(name))
        .unwrap_or_else(|| "Choose model".into());
    quiet_selector(
        world,
        ui.model,
        10,
        &format!("{model_title}  ⌄"),
        available,
        Size::Px(260.),
    );
    let mode_title = m
        .mode
        .as_ref()
        .and_then(|id| m.modes.iter().find(|(key, _, _)| key == id))
        .map(|(_, title, _)| title.as_str())
        .unwrap_or("Choose mode");
    quiet_selector(
        world,
        ui.mode,
        11,
        &format!("{mode_title}  ⌄"),
        available && m.chat.session().is_some(),
        Size::Px(130.),
    );
    quiet_selector(
        world,
        ui.depth,
        12,
        &format!(
            "Depth: {}  ⌄",
            ["Concise", "Standard", "Deep"][m.depth.min(2)]
        ),
        available,
        Size::Px(190.),
    );
    world.get_mut::<Label>(ui.model_caption).unwrap().text = m
        .selected
        .and_then(|i| m.models.get(i))
        .map(|(provider, _)| provider_caption(provider))
        .unwrap_or_default();
    let behavior_button = button(world, ui.footer, BEHAVIOR, "Behavior  ⌄", true);
    world.get_mut::<Layout>(behavior_button.root).unwrap().width = Size::Px(118.);
    let busy = m.chat.busy() || runtimes::busy(world);
    if busy {
        world.insert(send.root, Hidden);
    } else {
        world.remove::<Hidden>(send.root);
    }
    let stop = button(
        world,
        ui.controls,
        STOP,
        "■",
        m.chat.busy() || runtimes::busy(world),
    );
    round_send(world, stop, true);
    if m.chat.busy() || runtimes::busy(world) {
        world.remove::<Hidden>(stop.root);
    } else {
        world.insert(stop.root, Hidden);
    }
    let selected_session = m.chat.session();
    let enabled = selected_session.is_some() && !m.loading;
    for (key, title) in [
        (RENAME, "Rename"),
        (FORK, "Fork"),
        (PIN, "Pin"),
        (UNPIN, "Unpin"),
        (DELETE, "Delete"),
    ] {
        button(world, ui.session_actions, key, title, enabled);
    }
    let confirming = world.resource::<Model>().unwrap().delete_target.as_deref()
        == selected_session
        && selected_session.is_some();
    for (key, title) in [
        (DELETE_CONFIRM, "Confirm deletion"),
        (DELETE_CANCEL, "Keep chat"),
    ] {
        let h = button(world, ui.session_actions, key, title, confirming);
        if confirming {
            world.remove::<Hidden>(h.root);
        } else {
            world.insert(h.root, Hidden);
        }
    }
    let more = button(
        world,
        world.get::<Parent>(ui.sessions).unwrap().entity,
        MORE,
        "Load more chats",
        world.resource::<Model>().unwrap().session_cursor.is_some(),
    );
    if world.resource::<Model>().unwrap().session_cursor.is_some() {
        world.remove::<Hidden>(more.root);
    } else {
        world.insert(more.root, Hidden);
    }

    let sidebar_key = format!("{:?}{:?}", m.sessions, selected_session);
    let sidebar_changed = world.resource::<Model>().unwrap().sidebar_key != sidebar_key;
    world.resource_mut::<Model>().unwrap().sidebar_key = sidebar_key;
    if sidebar_changed {
        for (i, (id, title)) in m.sessions.iter().enumerate() {
            let key = SESSION + i as u64;
            let props = ButtonProps::new(
                if title.is_empty() {
                    "Untitled chat"
                } else {
                    title
                },
                HostIntent::Command(key),
            );
            let meta = NavigationMeta {
                selected: selected_session == Some(id),
                ..Default::default()
            };
            let h =
                controls::NavigationRow::mount(world, ui.sessions, UiId(key), props, meta).unwrap();
            world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
            world.get_mut::<Layout>(h.root).unwrap().height = Size::Px(34.);
            world.remove::<Hidden>(h.root);
        }
        // Keep keyed control ownership intact; hidden stale results cannot activate.
        for e in children_of(world, ui.sessions) {
            if let Some(UiId(key)) = rndr_ui::identity(world, e) {
                if key >= SESSION + m.sessions.len() as u64 {
                    world.insert(e, Hidden);
                }
            }
        }
    }
    let provider = m
        .chat
        .session()
        .and_then(|session| runtimes::conversation(world, session))
        .map(provider_timeline::project);
    timeline::sync(
        world,
        ui.timeline,
        provider.as_ref().or_else(|| m.chat.snapshot()),
    );
    let width = world
        .resource::<Viewport>()
        .map(|v| v.width)
        .unwrap_or(1200.);
    world.get_mut::<Layout>(ui.sidebar).unwrap().width = Size::Px(230.);
    let state = world.resource::<Model>().unwrap();
    let (details, filter, sidebar) = (
        state.details_open,
        state.model_search_open,
        state.sidebar_open,
    );
    for (entity, visible) in [
        (ui.details, details),
        (ui.behavior, state.behavior_open),
        (ui.filter_row, filter),
        (ui.sidebar, sidebar),
    ] {
        if visible {
            world.remove::<Hidden>(entity);
        } else {
            world.insert(entity, Hidden);
        }
    }
    world.get_mut::<Layout>(ui.compose).unwrap().width =
        Size::Px((width - if sidebar { 230. } else { 0. } - 24.).clamp(300., 700.));
    world.resource_mut::<Model>().unwrap().chat = m.chat;
}

// RNDR's bounded selectors deliberately accept at most 256 entries. Filter and
// page the full daemon catalog, retaining the active selection on every page.
fn model_options(
    models: &[(String, String)],
    selected: Option<usize>,
    query: &str,
    page: usize,
) -> (Vec<SelectOption>, usize, usize) {
    let matches = models
        .iter()
        .enumerate()
        .filter(|(_, (p, n))| format!("{p} {n}").to_lowercase().contains(query))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let pages = matches.len().div_ceil(255).max(1);
    let page = page.min(pages - 1);
    let mut ids = matches
        .into_iter()
        .skip(page * 255)
        .take(255)
        .collect::<Vec<_>>();
    if let Some(i) = selected.filter(|i| *i < models.len()) {
        if !ids.contains(&i) {
            ids.insert(0, i);
        }
    }
    (
        ids.into_iter()
            .map(|i| SelectOption::new(i as u64, models[i].1.clone()))
            .collect(),
        pages,
        page,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> App {
        let mut app = app();
        app.world_mut().insert_resource(Viewport {
            width: 1200.,
            height: 800.,
            ..Default::default()
        });
        app.update();
        app
    }
    fn idle_fixture() -> (App, tokio::sync::mpsc::UnboundedReceiver<Request>) {
        let mut app = fixture();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.commands = Some(tx);
        m.chat
            .hydrate("s".into(), "a".into(), vec![], None)
            .unwrap();
        m.models = vec![
            ("openai-codex".into(), "gpt-6.1-sol".into()),
            ("openrouter".into(), "another-model".into()),
        ];
        m.selected = Some(0);
        m.modes = vec![("coder".into(), "Coder".into(), true)];
        m.mode = Some("coder".into());
        app.update();
        app.update();
        (app, rx)
    }
    fn draft(app: &mut App, text: &str) {
        let id = app.world().resource::<Model>().unwrap().document;
        let doc = app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap();
        doc.commit(rndr_core::editing::SourceBatch {
            document: id,
            base: doc.revision(),
            origin: rndr_core::editing::EditOrigin::Host(99),
            changes: vec![rndr_core::editing::SourceChange {
                range: 0..doc.text().len(),
                replacement: text.into(),
            }],
        })
        .unwrap();
        app.update();
        app.update();
    }
    #[test]
    fn composer_has_cohesive_editor_and_secondary_controls_below_card() {
        let (mut app, _rx) = idle_fixture();
        for width in [1200., 760.] {
            app.world_mut().resource_mut::<Viewport>().unwrap().width = width;
            app.update();
            app.update();
            let w = app.world();
            let ui = *w.resource::<Ui>().unwrap();
            let editor = w.get::<Rect>(ui.composer.editor).unwrap();
            let mode = w.get::<Rect>(ui.mode).unwrap();
            let model = w.get::<Rect>(ui.model).unwrap();
            assert!(
                mode.y > model.y && model.y >= editor.y + editor.h,
                "editor={editor:?}, model={model:?}, mode={mode:?}"
            );
            assert!(
                editor.x + editor.w <= width && editor.w > 350.,
                "{editor:?}"
            );
            assert!(w.has::<ResolvedColor>(ui.composer.editor));
            assert_eq!(w.get::<Color>(ui.composer.editor), Some(&composer_color()));
            assert!(w.has::<Hidden>(ui.behavior));
        }
    }
    #[test]
    fn behavior_and_catalog_toggles_preserve_draft_document_and_undo() {
        let (mut app, _rx) = idle_fixture();
        draft(&mut app, "one\ntwo\nthree\nfour");
        let w = app.world();
        let ui = *w.resource::<Ui>().unwrap();
        let id = w.resource::<Model>().unwrap().document;
        let revision = w
            .resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .revision();
        assert!(w.get::<Rect>(ui.composer.editor).unwrap().h > 80.);
        action(app.world_mut(), BEHAVIOR);
        action(app.world_mut(), MODEL_SEARCH);
        app.update();
        app.update();
        let w = app.world();
        assert!(!w.has::<Hidden>(ui.behavior));
        assert!(!w.has::<Hidden>(ui.filter_row));
        assert_eq!(w.resource::<Model>().unwrap().document, id);
        assert_eq!(
            w.resource::<Ui>().unwrap().composer.editor,
            ui.composer.editor
        );
        assert_eq!(
            w.resource::<SourceDocuments>()
                .unwrap()
                .get(id)
                .unwrap()
                .revision(),
            revision
        );
        app.world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap()
            .undo()
            .unwrap();
        app.update();
        assert_eq!(
            app.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(id)
                .unwrap()
                .text(),
            ""
        );
    }
    #[test]
    fn quiet_model_trigger_keeps_native_popup_selection_and_send_gate() {
        let (mut app, _rx) = idle_fixture();
        let ui = *app.world().resource::<Ui>().unwrap();
        let send = button(app.world_mut(), ui.controls, SEND, "↑", false);
        assert!(!app.world().get::<Button>(send.root).unwrap().enabled);
        draft(&mut app, "Draft only; do not send");
        assert!(app.world().get::<Button>(send.root).unwrap().enabled);
        assert_eq!(
            try_activate(app.world_mut(), ui.model),
            ActivationResult::Accepted
        );
        app.update();
        app.update();
        let option = app
            .world()
            .entities_with::<Semantics>()
            .into_iter()
            .find(|e| {
                app.world()
                    .get::<Semantics>(*e)
                    .and_then(|s| s.name.as_deref())
                    == Some("another-model")
                    && app.world().has::<Button>(*e)
            })
            .expect("native popup option");
        assert_eq!(
            try_activate(app.world_mut(), option),
            ActivationResult::Accepted
        );
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Model>().unwrap().selected, Some(1));
        assert_eq!(
            app.world().get::<Label>(ui.model_caption).unwrap().text,
            "Using OpenRouter"
        );
        assert_eq!(
            app.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(app.world().resource::<Model>().unwrap().document)
                .unwrap()
                .text(),
            "Draft only; do not send"
        );
    }
    #[test]
    fn reopening_history_restores_model_when_catalog_arrives_late() {
        let mut app = fixture();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.chat.hydrate("s".into(), "a".into(), vec![serde_json::json!({
            "role":"assistant", "content":"Hello", "parts":[{"kind":"model_receipt","provider":"openai-codex","model":"gpt-6.1-sol"}]
        })], None).unwrap();
        m.models = vec![
            ("openrouter".into(), "aion".into()),
            ("openai-codex".into(), "gpt-6.1-sol".into()),
        ];
        m.selected = Some(0);
        restore_history_model(m);
        assert_eq!(m.selected, Some(1));
    }
    #[test]
    fn shell_has_sidebar_full_height_and_visible_composer() {
        let mut app = fixture();
        app.update();
        let w = app.world();
        let ui = w.resource::<Ui>().unwrap();
        let editor = w.get::<Rect>(ui.composer.editor).unwrap();

        assert!(editor.w > 300. && editor.h > 30., "{editor:?}");
        assert!(
            editor.x >= 260. && editor.y > 300. && editor.y + editor.h <= 800.,
            "{editor:?}"
        );
    }
    #[test]
    fn a_large_catalog_never_exceeds_rndr_native_selection_capacity() {
        let models = (0..600)
            .map(|i| ("fixture".into(), format!("model {i}")))
            .collect::<Vec<_>>();
        for page in 0..4 {
            let (options, _, _) = model_options(&models, Some(599), "", page);
            assert!(options.len() <= 256);
            assert!(options.iter().any(|o| o.id == OptionId(599)));
        }
    }
    #[test]
    fn stale_history_cannot_replace_the_selected_conversation() {
        let mut app = fixture();
        app.world_mut().resource_mut::<Model>().unwrap().generation = 3;
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 2,
                update: Update::History("old".into(), "Old".into(), vec![], None),
            },
        );
        assert!(
            app.world()
                .resource::<Model>()
                .unwrap()
                .chat
                .session()
                .is_none()
        );
    }
    #[test]
    fn canonical_send_request_serializes_without_a_runtime_json_guess() {
        let request = crate::chat::turn_request("session", "hello", "fixture", "model");
        let value = serde_json::to_value(&request).unwrap();
        assert!(value["stage_routing"]["orchestrator"].is_object());
        assert_eq!(value["persist_user_turn"], true);
    }
}

pub fn external_admitted(world: &mut World, session: &str) {
    let draft = world
        .resource_mut::<Model>()
        .unwrap()
        .external_drafts
        .remove(session);
    if let Some((id, revision)) = draft {
        let docs = world.resource_mut::<SourceDocuments>().unwrap();
        if docs.get(id).is_some_and(|d| d.revision() == revision) {
            let _ = docs.get_mut(id).unwrap().reload(revision, "");
        }
    }
}
pub fn runtime_event(world: &mut World, session: &str, command: medousa_app_core::ChatCommand) {
    let m = world.resource_mut::<Model>().unwrap();
    if let Err(error) = m.chat.dispatch(command) {
        if m.chat.session() == Some(session) {
            m.chat.status = error;
        }
    }
}
pub fn runtime_error(world: &mut World, session: &str, error: String) {
    let m = world.resource_mut::<Model>().unwrap();
    if m.chat.session() == Some(session) {
        m.chat.status = error;
    }
}
