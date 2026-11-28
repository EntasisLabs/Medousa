//! App-owned orchestration. RNDR owns controls/editing; app-core owns chat state.
#[cfg(test)]
#[path = "startup_tests.rs"]
mod startup_tests;
#[cfg(test)]
#[path = "workshop_tests.rs"]
mod workshop_tests;
use crate::{
    approvals, attachments, auxiliary,
    backend::{Command, Reply, Request, Update},
    chat::Chat,
    context, native_style as visual, provider_timeline, runtimes, timeline, workspace,
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
use std::sync::Arc;
#[path = "chat_sidebar.rs"]
mod chat_sidebar;
#[path = "window_chrome.rs"]
pub(crate) mod chrome;
#[path = "collaboration.rs"]
pub(crate) mod collaboration;
#[path = "command_palette.rs"]
mod command_palette;
#[path = "conversations.rs"]
pub(crate) mod conversations;
#[path = "model_picker.rs"]
mod model_picker;
#[path = "workshop_status.rs"]
pub(crate) mod workshop_status;

const BEHAVIOR: u64 = 33;
const TOGGLE_SEARCH: u64 = 34;
const DETAILS: u64 = 30;
const MODEL_SEARCH: u64 = 31;
const RUNTIME: u64 = 35;
const CONVERSATION_MENU: u64 = 36;
const SURFACE_SIDEBAR: u64 = 37;
const SIDEBAR: u64 = 32;
const NEW: u64 = 1;
const SEARCH: u64 = 2;
const RELOAD: u64 = 3;
const SEND: u64 = 4;
const STOP: u64 = 5;
const MODEL_PICKER: u64 = 10;
const SESSION: u64 = 1000;
const MORE: u64 = 8;
const PIN: u64 = 22;
const UNPIN: u64 = 26;

pub struct Model {
    collaboration: collaboration::State,
    pub chat: Chat,
    pub(crate) owner: String,
    pub commands: Option<tokio::sync::mpsc::UnboundedSender<Request>>,
    generation: u64,
    requested_session: Option<String>,
    loading: bool,
    models: Vec<(String, String)>,
    selected: Option<usize>,
    defaults: Option<medousa_types::TuiDefaults>,
    model_chosen: bool,
    depth_chosen: bool,
    defaults_pending: bool,
    pending_first_send: Option<(DocumentId, rndr_core::editing::Revision)>,
    pending_runtime_picker: bool,
    pending_drop_files: Vec<std::path::PathBuf>,
    sessions: Vec<(String, String)>,
    modes: Vec<(String, String, bool)>,
    mode: Option<String>,
    depth: usize,
    search_query: String,
    session_cursor: Option<String>,
    pending_media: Vec<medousa_types::MediaRef>,
    drafts: HashMap<String, DocumentId>,
    external_drafts: HashMap<String, (DocumentId, rndr_core::editing::Revision)>,
    approval_refresh: std::time::Instant,
    idle_refresh: std::time::Instant,
    idle_refresh_pending: bool,
    idle_refresh_delay: std::time::Duration,
    transcript_revision: u64,
    selectors_key: String,
    sidebar_key: String,
    details_open: bool,
    model_search_open: bool,
    sidebar_open: bool,
    sidebar_search_open: bool,
    document: DocumentId,
    pending_draft: Option<(DocumentId, rndr_core::editing::Revision)>,
}
#[cfg(test)]
impl Model {
    pub(crate) fn set_sessions_for_test(&mut self, sessions: Vec<(String, String)>) {
        self.sessions = sessions;
    }
}
impl Model {
    pub(crate) fn document_id(&self) -> DocumentId {
        self.document
    }
    fn new(document: DocumentId) -> Self {
        Model {
            collaboration: collaboration::State::default(),
            chat: Chat::connecting(),
            owner: String::new(),
            commands: None,
            generation: 0,
            requested_session: None,
            loading: false,
            models: vec![],
            selected: None,
            defaults: None,
            model_chosen: false,
            depth_chosen: false,
            defaults_pending: false,
            pending_first_send: None,
            pending_runtime_picker: false,
            pending_drop_files: vec![],
            sessions: vec![],
            modes: vec![],
            mode: Some("assistant".into()),
            depth: 1,
            search_query: String::new(),
            session_cursor: None,
            pending_media: vec![],
            drafts: HashMap::new(),
            external_drafts: HashMap::new(),
            approval_refresh: std::time::Instant::now(),
            idle_refresh: std::time::Instant::now(),
            idle_refresh_pending: false,
            idle_refresh_delay: std::time::Duration::from_secs(2),
            transcript_revision: 0,
            selectors_key: String::new(),
            sidebar_key: String::new(),
            details_open: false,
            model_search_open: false,
            sidebar_open: true,
            sidebar_search_open: false,
            document,
            pending_draft: None,
        }
    }
}
#[derive(Clone, Copy)]
struct Ui {
    content: Entity,
    sidebar: Entity,
    details: Entity,
    filter_row: Entity,
    compose: Entity,
    sessions: Entity,
    search: Entity,
    status: Entity,
    timeline: Entity,
    empty_presence: Entity,
    presence_tail: Entity,
    composer_parent: Entity,
    exchange: collaboration::View,
    composer: EditorSurfaceHandle,
    model: Entity,
    mode: Entity,
    depth: Entity,
    approvals: Entity,
    attachments: Entity,
    context: Entity,
    controls: Entity,
    footer: Entity,
    behavior: Entity,
    model_caption: Entity,
    model_filter: Entity,
}

pub fn app() -> App {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut()
        .insert_resource(model_picker::State::default());
    app.add_plugin(MarkdownPlugin);
    // Reuse RNDR retained rich Markdown for Notes alongside read-only rendering.
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
    app.add_plugin(rndr_term::TerminalPlugin);
    for stroke in [
        Keystroke::named(NamedKey::Enter),
        Keystroke::named(NamedKey::Enter).primary(),
    ] {
        app.world_mut()
            .resource_mut::<Keymap>()
            .unwrap()
            .bind(KeyBinding::new("chat.send", [stroke]).context("Chat composer"));
    }
    // Tests render the default theme, not whatever this machine has chosen.
    let medousa = crate::theme::resolve(&if cfg!(test) {
        crate::theme::default_id()
    } else {
        crate::theme::active_id()
    });
    app.world_mut().insert_resource(medousa.radii);
    let theme = UiTheme {
        density: Density::Compact,
        ramp: visual::ramp(),
        palette: medousa.palette,
        ..Default::default()
    };
    rndr_ui::apply_theme_bridge(app.world_mut(), theme);
    visual::install_zoom(app.world_mut());
    let mut docs = SourceDocuments::default();
    let document = docs.open("");
    app.world_mut().insert_resource(docs);
    app.world_mut().insert_resource(Model::new(document));
    app.world_mut().insert_resource(WindowConfig {
        title: "Medousa".into(),
        size: LogicalSize::new(1200., 850.),
        min_size: Some(LogicalSize::new(760., 540.)),
        titlebar: Some(Titlebar::new(chrome::HEIGHT).traffic_lights(16., 18.)),
        ..Default::default()
    });
    app.add_system(Schedule::Startup, setup);
    // Before layout's scroll pass, so the canvas pans and zooms with the wheel.
    app.add_system_front(Schedule::Input, crate::create::take_wheel);
    app.add_system(Schedule::Update, visual::reading_zoom);
    app.add_system(Schedule::Update, visual::sync_appearance);
    app.add_system(Schedule::Update, update);
    app.add_system(Schedule::Update, workshop_status::update);
    app.add_system(Schedule::Update, command_palette::update);
    app.add_system(Schedule::Update, crate::settings::update);
    app.add_system(Schedule::Update, crate::browser::update);
    app.add_system(Schedule::Layout, workspace::chrome);
    app.add_system(Schedule::Layout, crate::surfaces::chrome);
    app.add_system(Schedule::Layout, crate::calendar::chrome);
    app.add_system(Schedule::Layout, crate::artifacts::chrome);
    app.add_system(Schedule::Layout, timeline::chrome);
    app.add_system(Schedule::Layout, visual::document_layouts);
    app.add_system(Schedule::Layout, bound_composer_triggers);
    app.add_system(Schedule::Layout, visual::fit_labels);
    app.add_system(Schedule::Layout, visual::path_crumbs);
    // Before the band's pass: it re-hugs a scrolling sidebar's wrapper,
    // and a drag's width must be read first.
    app.add_system(Schedule::Layout, crate::sidebars::sync);
    app.add_system(Schedule::Layout, chrome::polish);
    app.add_system(Schedule::Layout, visual::action_icons);
    app.add_system(Schedule::Layout, crate::browser::layout);
    app.add_system(Schedule::Layout, visual::borders);
    app.add_system(Schedule::Layout, visual::quiet_chrome);
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
        SIDEBAR => Some("Toggle conversation sidebar".into()),
        SURFACE_SIDEBAR => Some("Toggle sidebar".into()),
        CONVERSATION_MENU => Some("Conversation actions".into()),
        DETAILS => Some("Context and sticky notes".into()),
        RUNTIME => Some("Choose runtime".into()),
        MODEL_PICKER => Some("Choose provider and model".into()),
        _ => None,
    };
    props.variant = if id == SEND || id == STOP {
        controls::ButtonVariant::Primary
    } else {
        controls::ButtonVariant::Quiet
    };
    let icon = match (id, title) {
        (SIDEBAR, "Chats") => Some((Icon::PanelLeft, "Toggle conversation sidebar")),
        (SURFACE_SIDEBAR, _) => Some((Icon::PanelLeft, "Toggle sidebar")),
        (CONVERSATION_MENU, _) => Some((Icon::Ellipsis, "Conversation actions")),
        (TOGGLE_SEARCH, _) => Some((Icon::Search, "Search conversations")),
        (NEW, "New chat") => Some((Icon::Plus, "New chat")),
        (40, "Split") => Some((Icon::PanelLeft, "Split conversation pane")),
        (41, "Join") => Some((Icon::PanelLeft, "Join conversation panes")),
        (SEND, _) => Some((Icon::ArrowUp, "Send message")),
        (STOP, _) => Some((Icon::Square, "Stop response")),
        (BEHAVIOR, _) => Some((Icon::SlidersHorizontal, "Behavior")),
        _ => None,
    };
    let h = if let Some((icon, name)) = icon {
        let handle = controls::IconButton::icon(world, parent, UiId(id), icon, name, props)
            .expect("stable icon control");
        world.get_mut::<LabelBox>(handle.label).unwrap().inset_x = 0.;
        visual::action_hint(world, handle.root, name);
        handle
    } else {
        controls::Button::mount(world, parent, UiId(id), props).expect("stable control")
    };
    world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
    if world
        .get::<Layout>(parent)
        .is_some_and(|l| l.axis == Axis::Row)
    {
        world.get_mut::<Layout>(h.root).unwrap().width = Size::Px(match id {
            SEND | STOP => 54.,
            DETAILS | SIDEBAR | SURFACE_SIDEBAR | CONVERSATION_MENU => {
                // Square, like the band's other icon buttons.
                if icon.is_some() {
                    visual::ACTION_SIZE
                } else {
                    64.
                }
            }
            40 | 41 | NEW | TOGGLE_SEARCH | BEHAVIOR => visual::ACTION_SIZE,
            MODEL_SEARCH | MODEL_PICKER => 150.,
            _ => 90.,
        });
    }
    h
}
fn plane(world: &mut World, entity: Entity, color: Token) {
    world.insert(
        entity,
        UiStyle {
            background: Some(color.into()),
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
            ..Layout::column()
        },
    );
    let body = chrome::mount(world, root);
    let chat_root = crate::surfaces::mount(world, body);
    workshop_status::mount(world, crate::surfaces::rail(world).expect("rail"));
    crate::surfaces::settings_to_foot(world);
    crate::bots::mount(world, root);
    command_palette::mount(world, chrome::band(world).expect("window band"));
    let (navigation_split, details_split) = workspace::mount(world, chat_root);
    let prefs = world.resource::<workspace::Workspace>().unwrap().prefs;
    let model = world.resource_mut::<Model>().unwrap();
    model.sidebar_open = prefs.sidebar_open;
    model.details_open = prefs.details_open;
    let sidebar = Surface::Navigation
        .mount(world, navigation_split.leading, UiId(1))
        .unwrap();
    *world.get_mut::<Layout>(sidebar.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    };
    *world.get_mut::<Layout>(sidebar.content).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        padding: 8.,
        gap: 4.,
        ..Layout::column()
    };
    visual::flat(world, sidebar.root, visual::navigation());
    world.insert(sidebar.root, visual::SidebarPane);
    // One inset, the content's: the header lines up with the titlebar row
    // and the other sidebars instead of stacking the surface's padding too.
    world.insert(sidebar.root, Padding::all(0.));
    let sidebar_header = container(
        world,
        Some(sidebar.content),
        Layout {
            width: Size::Fill,
            height: Size::Px(32.),
            gap: 8.,
            ..Layout::row()
        },
    );
    world.insert(
        sidebar_header,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let title = text(world, sidebar_header, "Chats", Typography::Label).unwrap();
    world.get_mut::<Layout>(title).unwrap().width = Size::Fill;
    button(world, sidebar_header, NEW, "New chat", true);
    button(world, sidebar_header, TOGGLE_SEARCH, "Search", true);
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
    world.insert(search, Hidden);
    plane(world, search, visual::rail());
    world.get_mut::<UiStyle>(search).unwrap().typography = Some(Typography::Label);
    world.insert(search, visual::SemanticBorder(visual::stroke()));
    world.insert(search, Radius(visual::CONTROL_RADIUS));
    world.insert(
        search,
        Border {
            color: visual::color(world, visual::stroke()),
            width: 1.,
        },
    );
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
    let details = container(
        world,
        Some(details_split.trailing),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 8.,
            gap: 6.,
            ..Layout::column()
        },
    );
    world.insert(details, Scroll::default());
    plane(world, details, visual::navigation());
    let dock_header = container(
        world,
        Some(details),
        Layout {
            width: Size::Fill,
            height: Size::Px(36.),
            gap: 4.,
            ..Layout::row()
        },
    );
    let dock_title = text(
        world,
        dock_header,
        "Context & sticky notes",
        Typography::Label,
    )
    .unwrap();
    world.get_mut::<Layout>(dock_title).unwrap().width = Size::Fill;
    button(world, dock_header, DETAILS, "×", true);
    let context = context::setup(world, details);
    runtimes::setup(world);
    let shared = Shared {
        sidebar: sidebar.root,
        sessions,
        search,
        details,
        context,
    };
    let parent = conversations::mount(world, details_split.leading);
    let ui = mount_view(world, parent, shared);
    world.insert_resource(ui);
}

#[derive(Clone, Copy)]
struct Shared {
    sidebar: Entity,
    sessions: Entity,
    search: Entity,
    details: Entity,
    context: Entity,
}
fn mount_view(world: &mut World, parent: Entity, shared: Shared) -> Ui {
    let Shared {
        sidebar,
        sessions,
        search,
        details,
        context,
    } = shared;
    let parent = container(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
    );
    let content = Surface::Content.mount(world, parent, UiId(2)).unwrap();
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
    visual::flat(world, content.root, visual::canvas());
    // Both lanes retain their entities and source documents across thread toggles.
    world.get_mut::<Layout>(content.content).unwrap().axis = Axis::Row;
    // A round's tool panel docks here, beside the conversation and composer.
    world.insert(content.content, timeline::ToolDock);
    let main = container(
        world,
        Some(content.content),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 4.,
            ..Layout::column()
        },
    );
    let exchange = collaboration::mount(world, main, content.content);
    let timeline = timeline::setup(world, main);
    // A separate retained surface keeps the timeline and its scroll position
    // intact while a genuinely empty, loaded conversation is displayed.
    let empty_presence = container(
        world,
        Some(main),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 12.,
            ..Layout::column()
        },
    );
    world.insert(empty_presence, Hidden);
    container(
        world,
        Some(empty_presence),
        Layout {
            height: Size::Fill,
            ..Layout::column()
        },
    );
    let ask = text(world, empty_presence, presence_ask(), Typography::Reading).unwrap();
    world.insert(ask, AlignSelf(Items::Center));
    world.insert(
        ask,
        visual::FitLabel {
            label: ask,
            extra: 0.,
            min: 0.,
            max: 360.,
        },
    );
    world.insert(
        ask,
        UiStyle {
            foreground: Some(visual::text().into()),
            typography: Some(Typography::Heading),
            ..Default::default()
        },
    );
    let approvals = approvals::setup(world, main);
    approvals::thread_host(world, approvals, exchange.approvals);
    let composer_row = container(
        world,
        Some(main),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            padding: 16.,
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
            width: Size::Px(visual::COLUMN),
            height: Size::Hug,
            gap: 4.,
            ..Layout::column()
        },
    );
    let compose = Surface::Raised
        .mount(world, composer_shell, UiId(3))
        .unwrap();
    // Earlier collaborations: a right-aligned chip under the card.
    collaboration::place_chip(world, &exchange, composer_shell);
    *world.get_mut::<Layout>(compose.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Hug,
        ..Layout::column()
    };
    *world.get_mut::<Layout>(compose.content).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Hug,
        padding: 8.,
        gap: 4.,
        ..Layout::column()
    };
    world.insert(
        compose.root,
        UiStyle {
            background: Some(composer_color().into()),
            padding: Some(Space::Zero),
            gap: Some(Space::Zero),
            ..Default::default()
        },
    );
    world.insert(compose.root, Radius(visual::COMPOSER_RADIUS));
    world.insert(compose.root, visual::SemanticBorder(visual::stroke()));
    world.insert(
        compose.root,
        Border {
            color: visual::color(world, visual::stroke()),
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
            height: Size::Px(32.),
            padding: 0.,
            ..Layout::column()
        },
    );
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
    world.insert(
        controls,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let attachments = attachments::setup(world, controls, attachment_tray);
    world.get_mut::<Layout>(attachments).unwrap().width = Size::Px(visual::ACTION_SIZE);
    // One row of controls inside the card, as every polished composer does:
    // attach · agent · mode · behavior ··· model · send.
    let runtime_trigger = button(world, controls, RUNTIME, "Medousa  ⌄", true);
    fit_trigger(world, runtime_trigger);
    let mode = SingleSelect::mount(
        world,
        controls,
        UiId(11),
        SelectionProps::new("Mode"),
        &[],
        None,
    )
    .unwrap()
    .root;
    // Depth and model search sit behind one quiet icon, not a third label.
    let behavior_trigger = button(world, controls, BEHAVIOR, "Behavior", true);
    container(
        world,
        Some(controls),
        Layout {
            width: Size::Fill,
            ..Layout::row()
        },
    );
    // Provider caption and model read as one trailing line beside Send.
    let model_stack = container(
        world,
        Some(controls),
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 2.,
            ..Layout::row()
        },
    );
    world.insert(
        model_stack,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let model_caption = text(world, model_stack, "Choose a model", Typography::Caption).unwrap();
    world.insert(
        model_caption,
        UiStyle {
            foreground: Some(visual::secondary().into()),
            ..Default::default()
        },
    );
    world.get_mut::<Layout>(model_caption).unwrap().padding = 0.;
    // The model name alone identifies the choice; the provider caption stays
    // current for the model picker but does not take a second label here.
    world.insert(model_caption, Hidden);
    world.insert(
        model_caption,
        visual::FitLabel {
            label: model_caption,
            extra: 0.,
            min: 0.,
            max: 160.,
        },
    );
    let model = button(world, model_stack, MODEL_PICKER, "Choose model  ⌄", true).root;
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
    world.insert(
        footer,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    // The footer is now only the status sentence, read above the card.
    if let Some(children) = world.get_mut::<Children>(composer_shell) {
        children.list.retain(|e| *e != footer);
        children.list.insert(0, footer);
    }
    let status_slot = container(
        world,
        Some(footer),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            ..Layout::row()
        },
    );
    world.insert(
        status_slot,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    // Detached native popup: opening it must not relayout the writing lane.
    let behavior = container(
        world,
        None,
        Layout {
            width: Size::Px(260.),
            height: Size::Hug,
            padding: 8.,
            gap: 4.,
            ..Layout::column()
        },
    );
    world.insert(behavior, Hidden);
    world.insert(
        behavior,
        AnchoredPopup {
            anchor: behavior_trigger.root,
            max_height: 240.,
            closed: |_, _| {},
        },
    );
    world.insert(behavior, PopupPlacement::Below);
    visual::plane(world, behavior, visual::raised());
    world.insert(behavior, Radius(visual::CONTROL_RADIUS));
    world.insert(
        behavior,
        Border {
            color: visual::color(world, visual::stroke()),
            width: 1.,
        },
    );
    world.insert(behavior, visual::SemanticBorder(visual::stroke()));
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
        Some(compose.content),
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
    button(world, filter_row, MODEL_PICKER, "Choose provider…", true);
    // Connection and turn state read as one quiet sentence above the card,
    // not as another control row under the writing lane.
    let status = text(world, status_slot, "Connecting…", Typography::Caption).unwrap();
    world.get_mut::<Layout>(status).unwrap().width = Size::Fill;
    world.get_mut::<Layout>(status).unwrap().padding = 0.;
    world.get_mut::<Label>(status).unwrap().overflow = TextOverflow::Ellipsis;
    world.insert(
        status,
        UiStyle {
            foreground: Some(visual::secondary().into()),
            ..Default::default()
        },
    );
    // Balances the presence greeting so it and the composer sit together at
    // the optical center of an empty conversation.
    let presence_tail = container(
        world,
        Some(main),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
    );
    world.insert(presence_tail, Hidden);
    Ui {
        content: parent,
        presence_tail,
        sidebar,
        details,
        filter_row,
        compose: composer_shell,
        sessions,
        search,
        status,
        timeline,
        empty_presence,
        exchange,
        composer_parent,
        composer,
        model,
        mode,
        depth,
        approvals,
        attachments,
        context,
        controls,
        footer,
        behavior,
        model_caption,
        model_filter,
    }
}
fn presence_ask() -> &'static str {
    use chrono::Timelike;
    match chrono::Local::now().hour() {
        0..=4 => "What are we doing past midnight?",
        5..=11 => "What are we doing this morning?",
        12..=16 => "What are we doing today?",
        17..=20 => "What are we doing this evening?",
        _ => "What are we doing tonight?",
    }
}
fn mount_composer(world: &mut World, parent: Entity, document: DocumentId) -> EditorSurfaceHandle {
    let mut props = EditorSurfaceProps::new(document, "Message Medousa");
    props.gutter = false;
    props.scrollbar = true;
    props.description = "Enter sends your message. Shift+Enter inserts a new line.".into();
    let h = EditorSurface::mount(world, parent, UiId(1), props).expect("live source document");
    world.insert(
        h.editor,
        UiStyle {
            background: Some(composer_color().into()),
            typography: Some(Typography::Label),
            ..Default::default()
        },
    );
    for child in children_of(world, h.root) {
        if world.has::<Scrollbar>(child) {
            world.get_mut::<Layout>(child).unwrap().width = Size::Px(8.);
            plane(world, child, composer_color());
        }
    }
    // Core editor paint otherwise supplies its own gray input background.
    world.insert(h.editor, ResolvedColor);
    world.insert(h.editor, Context::new("Chat composer"));
    // Leave Color to UiStyle's resolver: an authored primitive Color would
    // pin the mount-time appearance and block subsequent theme changes.
    world.insert(h.editor, Placeholder::new("Message Medousa…"));
    world.get_mut::<Label>(h.editor).unwrap().overflow = TextOverflow::Wrap;
    h
}
// Height measurement is bounded independently from the retained source document.
// A huge paste must not reshape the complete draft on every frame.
struct ComposerMeasure {
    document: DocumentId,
    revision: rndr_core::editing::Revision,
    width: f32,
    style: TextStyle,
    engine: Arc<dyn TextEngine>,
    viewport: f32,
}
impl Component for ComposerMeasure {}
fn sync_composer_height(
    world: &mut World,
    handle: EditorSurfaceHandle,
    parent: Entity,
    document: DocumentId,
) {
    let Some(rect) = world.get::<Rect>(handle.editor).copied() else {
        return;
    };
    let Some(doc) = world
        .resource::<SourceDocuments>()
        .and_then(|d| d.get(document))
    else {
        return;
    };
    let revision = doc.revision();
    let style = style_for(world, handle.editor);
    let engine = text_engine(world);
    let viewport = world.resource::<Viewport>().map_or(800., |v| v.height);
    if world.get::<ComposerMeasure>(handle.root).is_some_and(|m| {
        m.document == document
            && m.revision == revision
            && m.width == rect.w
            && m.style == style
            && Arc::ptr_eq(&m.engine, &engine)
            && m.viewport == viewport
    }) {
        return;
    }
    let text = doc.text();
    let mut end = text.len().min(4096);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let width = (rect.w - 2. * TEXT_ORIGIN_X).max(engine.unit_advance(&style));
    let line = engine.layout("Mg", &style, &[], width).height.max(20.);
    let max = (10. * line + 16.).min((viewport * 0.35).max(32.));
    let height = if end < text.len() {
        max
    } else {
        (engine.layout(&text[..end], &style, &[], width).height + 16.).clamp(32., max)
    };
    world.get_mut::<Layout>(parent).unwrap().height = Size::Px(height);
    world.insert(
        handle.root,
        ComposerMeasure {
            document,
            revision,
            width: rect.w,
            style,
            engine,
            viewport,
        },
    );
}
fn composer_color() -> Token {
    visual::raised()
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
fn quiet_selector(world: &mut World, root: Entity, id: u64, title: &str, enabled: bool, max: f32) {
    // Retain RNDR's selector event routing/popup and accessible name. Only its
    // trigger presentation changes; do not traverse/style popup option rows.
    let name = world.get::<Semantics>(root).and_then(|s| s.name.clone());
    let parent = world.get::<Parent>(root).unwrap().entity;
    let mut props = ButtonProps::new(title, HostIntent::Command(id));
    props.variant = controls::ButtonVariant::Quiet;
    props.enabled = enabled;
    props.name = name;
    let handle = controls::Button::mount(world, parent, UiId(id), props).unwrap();
    // Trigger width follows its measured title, so footers read as one line.
    world.insert(
        root,
        visual::FitLabel {
            label: handle.label,
            extra: 4.,
            min: 48.,
            max,
        },
    );
    world.get_mut::<Label>(handle.label).unwrap().overflow = TextOverflow::Ellipsis;
    // Selectors remain visually quiet even while disabled during a turn.
    // Keep the native outer focus ring and the popup untouched.
    // RNDR owns hover/pressed/disabled/focus layers; do not flatten them.
}
/// Labeled triggers share the composer's one row with attach, behavior and
/// send. Bound them by the row each layout so a narrow window ellipsizes
/// names instead of pushing Send out of the card.
/// Hidden by [`bound_composer_triggers`] for width alone; restored when room returns.
struct CompactDropped;
impl Component for CompactDropped {}
fn bound_composer_triggers(world: &mut World) {
    let Some(ui) = world.resource::<Ui>().copied() else {
        return;
    };
    let Some(row) = world.get::<Rect>(ui.controls).copied() else {
        return;
    };
    let gap = world.get::<Layout>(ui.controls).map_or(0., |l| l.gap);
    // What attach, behavior, stop/send and the gaps between visible controls
    // leave over; the stretching spacer takes none of it. Triggers this pass
    // dropped still count, so dropping one never frees the room to restore it.
    let visible = children_of(world, ui.controls)
        .into_iter()
        .filter(|&e| !world.has::<Hidden>(e) || world.has::<CompactDropped>(e))
        .collect::<Vec<_>>();
    let fixed: f32 = visible
        .iter()
        .filter(|&&e| !world.has::<visual::FitLabel>(e))
        .filter(|&&e| {
            world
                .get::<Layout>(e)
                .is_some_and(|l| l.width != Size::Fill)
        })
        .filter(|&&e| !descendant(world, ui.model, e))
        .filter_map(|&e| world.get::<Rect>(e).map(|r| r.w))
        .sum();
    let available = (row.w - fixed - gap * visible.len().saturating_sub(1) as f32).max(0.);
    // The model selector sits in its own stack inside the row.
    let triggers = visible
        .into_iter()
        .chain(std::iter::once(ui.model))
        .filter(|&e| world.has::<visual::FitLabel>(e))
        .collect::<Vec<_>>();
    // Natural width: measured label plus the trigger's own chrome.
    const CHROME: f32 = 24.;
    let engine = text_engine(world);
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let natural = triggers
        .iter()
        .map(|&e| {
            let label = world.get::<visual::FitLabel>(e).unwrap().label;
            let text = world
                .get::<Label>(label)
                .map(|l| l.text.clone())
                .unwrap_or_default();
            let style = world
                .get::<TextStyle>(label)
                .cloned()
                .unwrap_or_else(|| theme.typography(Typography::Label));
            engine.measure(&text, &style, &[], f32::INFINITY).width + CHROME
        })
        .collect::<Vec<_>>();
    // A narrow pane drops whole triggers before shrinking names to "M…":
    // the agent chip first (rarely changed per message), then mode. The
    // model and send always stay.
    let runtime = triggers.iter().position(|&e| {
        world
            .get::<Semantics>(e)
            .is_some_and(|s| s.name.as_deref() == Some("Choose runtime"))
    });
    let mode = triggers.iter().position(|&e| e == ui.mode);
    let mut total: f32 = natural.iter().sum();
    let mut dropped = vec![];
    for i in [runtime, mode].into_iter().flatten() {
        // Mild crowding still ellipsizes; only real squeeze drops a trigger.
        if total > available * 1.25 && triggers.len() - dropped.len() > 1 {
            total -= natural[i];
            dropped.push(i);
        }
    }
    for (i, &e) in triggers.iter().enumerate() {
        if dropped.contains(&i) {
            if !world.has::<CompactDropped>(e) {
                world.insert(e, CompactDropped);
                world.insert(e, Hidden);
            }
        } else if world.has::<CompactDropped>(e) {
            world.remove::<CompactDropped>(e);
            world.remove::<Hidden>(e);
        }
    }
    // Everything fits: no trigger is capped. Otherwise each gives up space in
    // proportion to its size, so one long name doesn't starve the others.
    for (&e, &width) in triggers.iter().zip(&natural) {
        let cap = if total <= available || total <= 0. {
            260.
        } else {
            available * width / total
        };
        let fit = world.get_mut::<visual::FitLabel>(e).unwrap();
        fit.max = cap.clamp(fit.min, 260.);
    }
}
fn descendant(world: &World, mut entity: Entity, ancestor: Entity) -> bool {
    while let Some(parent) = world.get::<Parent>(entity).map(|p| p.entity) {
        if parent == ancestor {
            return true;
        }
        entity = parent;
    }
    false
}
fn fit_trigger(world: &mut World, handle: controls::ControlHandle) {
    world.insert(
        handle.root,
        visual::FitLabel {
            label: handle.label,
            extra: 4.,
            min: 48.,
            max: 200.,
        },
    );
}
fn round_send(world: &mut World, handle: controls::ControlHandle, enabled: bool) {
    let mut nodes = vec![handle.root];
    while let Some(node) = nodes.pop() {
        // Shape the existing RNDR control primitives, preserving its focus,
        // hover, disabled state and activation semantics.
        let radius = if node == handle.root { 16. } else { 14. };
        world.insert(node, Radius(radius));
        if node != handle.label {
            world.get_mut::<Layout>(node).unwrap().height = Size::Fill;
        }
        nodes.extend(children_of(world, node));
    }
    *world.get_mut::<Layout>(handle.root).unwrap() = Layout {
        width: Size::Px(32.),
        height: Size::Px(32.),
        padding: 2.,
        ..Layout::column()
    };
    // Preserve the native primary recipe and its state feedback.
    let _ = enabled;
    // Fill the circular face with centered label content, rather than using
    // a short Hug-height face (which produces an over-rounded diamond).
    world.get_mut::<Layout>(handle.label).unwrap().height = Size::Fill;
    if let Some(metrics) = world.get_mut::<LabelBox>(handle.label) {
        metrics.inset_x = 4.;
        metrics.inset_y = 2.;
    }
}
fn depth_options() -> Vec<SelectOption> {
    ["Concise", "Standard", "Deep"]
        .iter()
        .enumerate()
        .map(|(i, s)| SelectOption::new(i as u64, *s))
        .collect()
}
pub(crate) fn link_notice(world: &mut World, session: &str, message: &str) {
    conversations::with_session(world, session, |world| {
        world.resource_mut::<Model>().unwrap().chat.status = message.into();
    });
}
fn dispatch(world: &mut World, command: Command) {
    if world.resource::<crate::workshops::Workshops>().is_some() {
        match &command {
            Command::List(query) => {
                crate::workshops::dispatch_list(world, query.clone(), false);
                return;
            }
            Command::More { query, .. } => {
                crate::workshops::dispatch_list(world, query.clone(), true);
                return;
            }
            _ => {}
        }
        let target = command
            .session_owner()
            .unwrap_or_else(|| world.resource::<Model>().unwrap().owner.clone());
        let tx = crate::workshops::commands(world, &target);
        let m = world.resource_mut::<Model>().unwrap();
        if tx.is_none_or(|tx| {
            tx.send(Request {
                generation: m.generation,
                command,
            })
            .is_err()
        }) {
            m.loading = false;
            m.pending_first_send = None;
            m.pending_runtime_picker = false;
            m.pending_drop_files.clear();
            m.chat.status = "Owning workshop unavailable. Reconnect that source; this item cannot use another daemon.".into();
        }
        return;
    }
    let m = world.resource_mut::<Model>().unwrap();
    if m.commands.as_ref().is_none_or(|tx| {
        tx.send(Request {
            generation: m.generation,
            command,
        })
        .is_err()
    }) {
        m.loading = false;
        m.pending_first_send = None;
        m.pending_runtime_picker = false;
        m.pending_drop_files.clear();
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
    // Promote the initial draft in place; do not remount or lose its text/focus.
    let initial = world.resource::<Model>().unwrap().chat.session().is_none()
        && world
            .resource::<Model>()
            .unwrap()
            .requested_session
            .is_none();
    let doc = known.unwrap_or_else(|| {
        if initial {
            old
        } else {
            world.resource_mut::<SourceDocuments>().unwrap().open("")
        }
    });
    if doc == old {
        world
            .resource_mut::<Model>()
            .unwrap()
            .drafts
            .insert(session.into(), doc);
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
// The SDK has turn streams but no session-ledger notification stream. Poll only
// a selected, settled conversation, with one bounded read outstanding. A revision
// fence prevents an older read from overwriting subsequent admission/live text.
fn refresh_idle_chat(world: &mut World) {
    let m = world.resource_mut::<Model>().unwrap();
    if m.loading
        || m.chat.busy()
        || m.chat.uncertain
        || (m.idle_refresh_pending && m.idle_refresh.elapsed() < std::time::Duration::from_secs(15))
        || m.idle_refresh.elapsed() < m.idle_refresh_delay
    {
        return;
    }
    let Some(session) = m.chat.session().map(str::to_owned) else {
        return;
    };
    m.idle_refresh = std::time::Instant::now();
    m.idle_refresh_pending = true;
    let revision = m.transcript_revision;
    dispatch(world, Command::Refresh(session, revision));
}

fn update(world: &mut World) {
    crate::workshops::tick(world);
    auxiliary::tick(world);
    for reply in events::<Reply>(world).to_vec() {
        apply_reply(world, reply);
    }
    crate::navigation::tick(world);
    crate::multiplexer::input(world);
    crate::terminal::tick(world);
    refresh_idle_chat(world);
    crate::surfaces::remember_focus(world);
    let chat_active = matches!(crate::surfaces::effective(world), crate::surfaces::SurfaceId::Chat | crate::surfaces::SurfaceId::Bots);
    if chat_active {
        conversations::focus_from_input(world);
    }
    let ui = *world.resource::<Ui>().unwrap();
    if !chat_active {
        close_popup(world, ui.behavior, false);
    }
    model_picker::update(world);
    if chat_active
        && world
            .resource::<KeymapFired>()
            .is_some_and(|k| k.actions.iter().any(|a| a == "chat.send"))
    {
        action(world, SEND);
    }
    for event in events::<SelectionChange>(world)
        .to_vec()
        .into_iter()
        .filter(|_| chat_active)
    {
        conversations::focus_entity(world, event.control);
        let ui = *world.resource::<Ui>().unwrap();
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
        if event.control == ui.depth && id < 3 {
            m.depth = id;
            m.depth_chosen = true;
        }
        if event.control == ui.mode && id < m.modes.len() && m.modes[id].2 {
            if let (Some(session), Ok(mode)) = (
                m.chat.session().map(str::to_owned),
                serde_json::from_value(serde_json::Value::String(m.modes[id].0.clone())),
            ) {
                m.loading = true;
                dispatch(world, Command::SetMode(session, mode));
            } else if m.chat.session().is_none() {
                // Draft tabs have no server identity yet. Persist this choice
                // during creation, before the first turn can be admitted.
                m.mode = Some(m.modes[id].0.clone());
            }
        }
    }
    if chat_active {
        conversations::tab_events(world);
        open_pending_runtime_picker(world);
        composer_file_drops(world);
    }
    // Canonical FindBar consumes its source-edit intents before app routing.
    if crate::surfaces::effective(world) == crate::surfaces::SurfaceId::Code {
        crate::code::keyboard(world);
    }
    if crate::surfaces::effective(world) == crate::surfaces::SurfaceId::Notes {
        crate::notes::keyboard(world);
    }
    rndr_ui::source_tools::apply_find_bars(world);
    for intent in take_action_intents(world) {
        crate::multiplexer::focus_entity(world, intent.entity);
        let host = controls::host_intent(world, &intent);
        if host.as_ref().is_some_and(|h| {
            crate::bots::action(world, h)
                || crate::workshops::action(world, h)
                || crate::surfaces::action(world, h)
                || workshop_status::action(world, h)
                || crate::multiplexer::action(world, h)
        }) {
            continue;
        }
        // The band's sidebar toggle belongs to the window, not the view under
        // it: handle it before the active destination claims the intent.
        if host == Some(HostIntent::Command(SURFACE_SIDEBAR)) {
            crate::sidebars::toggle(world);
            continue;
        }
        match crate::surfaces::effective(world) {
            crate::surfaces::SurfaceId::Notes => {
                if let Some(host) = host {
                    crate::notes::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Code => {
                if let Some(host) = host {
                    crate::code::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Reviews => {
                if let Some(host) = host {
                    crate::reviews::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Calendar => {
                if let Some(host) = host {
                    crate::calendar::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Create => {
                if let Some(host) = host {
                    crate::create::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Settings => {
                if let Some(host) = host {
                    crate::settings::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Peers => {
                if let Some(host) = host {
                    crate::peers::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Artifacts => {
                if let Some(host) = host {
                    crate::artifacts::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Terminal => {
                if let Some(host) = host {
                    crate::terminal::action(world, &host);
                }
                continue;
            }
            crate::surfaces::SurfaceId::Workspace | crate::surfaces::SurfaceId::Web => continue,
            crate::surfaces::SurfaceId::Chat | crate::surfaces::SurfaceId::Bots => {}
        }
        conversations::focus_entity(world, intent.entity);
        if approvals::action(world, &intent) || attachments::action(world, &intent) {
            continue;
        }
        if collaboration::action(world, &intent) {
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
                if !conversations::action(world, id) {
                    action(world, id);
                }
            }
        }
    }
    if crate::surfaces::effective(world) == crate::surfaces::SurfaceId::Chat
        && events::<Submitted>(world)
            .iter()
            .any(|s| s.entity == ui.search)
    {
        action(world, SEARCH);
    }
    if chat_active
        && events::<Submitted>(world)
            .iter()
            .any(|s| s.entity == ui.model_filter)
    {
        model_picker::open(world);
    }
    if let Some(error) = world
        .resource_mut::<auxiliary::Auxiliary>()
        .and_then(|a| a.error.take())
    {
        world.resource_mut::<Model>().unwrap().chat.status = error;
    }
    // Hidden chat transport/model updates above remain live; presentation catches
    // up on return instead of reconciling all chat controls on every Code keystroke.
    if matches!(
        crate::surfaces::active(world),
        crate::surfaces::SurfaceId::Chat | crate::surfaces::SurfaceId::Bots | crate::surfaces::SurfaceId::Workspace
    ) {
        conversations::sync_all(world);
    }
    crate::notes::tick(world);
    crate::code::sync(world);
    crate::artifacts::sync(world);
    crate::reviews::sync(world);
    crate::calendar::sync(world);
    crate::create::sync(world);
    crate::settings::sync(world);
    crate::peers::sync(world);
    crate::bots::sync(world);
    crate::multiplexer::sync(world);
    auxiliary::dispatch(world);
    // Every adapter above reads canonical SourceSession text/revision, not its
    // notification queue. Acknowledge after capture, including parked drafts:
    // RNDR intentionally refuses input once an undrained queue reaches 256.
    if let Some(documents) = world.resource_mut::<SourceDocuments>() {
        let ids: Vec<_> = documents.ids().collect();
        for id in ids {
            documents.get_mut(id).unwrap().drain_events().for_each(drop);
        }
    }
}
fn action(world: &mut World, id: u64) {
    if chat_sidebar::action(world, id) {
        return;
    }
    let ui = *world.resource::<Ui>().unwrap();
    match id {
        BEHAVIOR => {
            if world.has::<Hidden>(ui.behavior) {
                if open_popup(world, ui.behavior) {
                    request_focus(world, ui.depth);
                }
            } else {
                close_popup(world, ui.behavior, true);
            }
        }
        DETAILS => {
            let m = world.resource_mut::<Model>().unwrap();
            m.details_open = !m.details_open;
            if m.details_open {
                context::expand(world);
            }
        }
        CONVERSATION_MENU => {
            if let Some(anchor) = chrome::conversation_button(world) {
                chat_sidebar::conversation_menu(world, anchor);
            }
        }
        RUNTIME => {
            let m = world.resource::<Model>().unwrap();
            if m.loading || m.chat.busy() || m.chat.uncertain {
                return;
            }
            let session = world
                .resource::<Model>()
                .unwrap()
                .chat
                .session()
                .map(str::to_owned);
            if let Some(session) = session {
                let label = format!("{}  ⌄", runtimes::label(world, Some(&session)));
                let chip = button(world, ui.controls, RUNTIME, &label, true);
                runtimes::open(world, chip.root, &session);
            } else {
                create_initial_conversation(world, true);
            }
        }
        MODEL_SEARCH => {
            close_popup(world, ui.behavior, true);
            let m = world.resource_mut::<Model>().unwrap();
            m.model_search_open = !m.model_search_open;
        }
        TOGGLE_SEARCH => {
            let m = world.resource_mut::<Model>().unwrap();
            m.sidebar_search_open = !m.sidebar_search_open;
            if m.sidebar_search_open {
                world.remove::<Hidden>(ui.search);
                request_focus(world, ui.search);
            } else {
                // A filtered list must never silently remain filtered.
                world.get_mut::<Label>(ui.search).unwrap().text.clear();
                world.insert(ui.search, Hidden);
                request_focus(world, ui.composer.editor);
            }
        }
        SIDEBAR => {
            let m = world.resource_mut::<Model>().unwrap();
            m.sidebar_open = !m.sidebar_open;
        }
        SURFACE_SIDEBAR => crate::sidebars::toggle(world),
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
        MODEL_PICKER => model_picker::open(world),
        SEARCH => {
            let query = world.get::<Label>(ui.search).unwrap().text.clone();
            world.resource_mut::<Model>().unwrap().search_query = query.trim().to_owned();
            dispatch(world, Command::List(query));
        }
        NEW | RELOAD | SESSION.. => {
            conversations::navigate(world, id);
        }
        SEND => {
            if world.has::<Hidden>(ui.exchange.main) {
                return;
            }
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
            send_native(world);
        }
        STOP => {
            if world.has::<Hidden>(ui.exchange.main) {
                return;
            }
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
// Called only for the focused route, not while projecting a parked reply/view.
/// Positioned drops target only the composer. RNDR's current desktop binding
/// delivers path-only reports: on those platforms the visible Chat window is
/// the drop target. Never synthesize a drop position from the last (possibly
/// stale) mouse event.
fn composer_file_drops(world: &mut World) {
    let mut batches: Vec<(Entity, Vec<std::path::PathBuf>)> = vec![];
    for drop in events::<FileDrop>(world) {
        if let Some(target) = conversations::drop_target(world, drop.position) {
            if let Some((_, files)) = batches.iter_mut().find(|(e, _)| *e == target) {
                files.push(drop.path.clone());
            } else {
                batches.push((target, vec![drop.path.clone()]));
            }
        }
    }
    for (target, files) in batches {
        conversations::focus_entity(world, target);
        // IME composition can refuse a pane switch. Never redirect that drop.
        if world.resource::<Ui>().unwrap().compose != target {
            continue;
        }
        admit_composer_files(world, files);
    }
}
fn admit_composer_files(world: &mut World, files: Vec<std::path::PathBuf>) {
    let m = world.resource::<Model>().unwrap();
    if m.loading || m.chat.uncertain {
        world.resource_mut::<Model>().unwrap().chat.status =
            "Wait for the conversation to finish connecting, then drop the files again.".into();
        return;
    }
    if !matches!(runtimes::active(world), runtimes::ActiveRuntime::Native) {
        world.resource_mut::<Model>().unwrap().chat.status =
            "File attachments are not supported by this runtime. Choose Medousa to attach files."
                .into();
        return;
    }
    if let Some(session) = m.chat.session().map(str::to_owned) {
        if let Err(error) = attachments::queue_files(world, &session, files) {
            world.resource_mut::<Model>().unwrap().chat.status = error;
        }
    } else if files.len() > attachments::max_files() {
        world.resource_mut::<Model>().unwrap().chat.status =
            "Attach at most 10 files per message.".into();
    } else if m.commands.is_none() || m.requested_session.is_some() {
        world.resource_mut::<Model>().unwrap().chat.status =
            "Connect the owning workshop before attaching files.".into();
    } else {
        world.resource_mut::<Model>().unwrap().pending_drop_files = files;
        create_initial_conversation(world, false);
    }
}

fn open_pending_runtime_picker(world: &mut World) {
    let m = world.resource_mut::<Model>().unwrap();
    if !m.pending_runtime_picker || m.loading || m.chat.uncertain {
        return;
    }
    let Some(session) = m.chat.session().map(str::to_owned) else {
        return;
    };
    runtimes::show(world, Some(&session));
    let ui = *world.resource::<Ui>().unwrap();
    let chip = button(world, ui.controls, RUNTIME, "Medousa  ⌄", true);
    if runtimes::open(world, chip.root, &session) {
        world
            .resource_mut::<Model>()
            .unwrap()
            .pending_runtime_picker = false;
    }
}
// Runtime choice also needs a server session, but never implies sending a draft.
// Reuse the initial route, editor and selected mode rather than opening a new tab.
fn create_initial_conversation(world: &mut World, runtime_picker: bool) {
    let m = world.resource_mut::<Model>().unwrap();
    if m.loading || m.commands.is_none() || m.requested_session.is_some() {
        return;
    }
    m.pending_runtime_picker = runtime_picker;
    m.loading = true;
    let mode = m.mode.as_deref().unwrap_or("assistant");
    let mode = serde_json::from_value(serde_json::Value::String(mode.into()))
        .unwrap_or(medousa_types::AgentModeId::Assistant);
    let command = if mode == medousa_types::AgentModeId::Assistant {
        Command::New
    } else {
        Command::NewWithMode(mode)
    };
    dispatch(world, command);
}
fn send_native(world: &mut World) {
    let m = world.resource::<Model>().unwrap();
    if m.defaults_pending {
        return;
    }
    if m.loading || m.chat.busy() || m.chat.uncertain {
        return;
    }
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
    if doc.text().trim().is_empty() && m.chat.session().is_none() {
        return;
    }
    let Some(session) = m.chat.session() else {
        if m.commands.is_none() || m.requested_session.is_some() {
            return;
        }
        world.resource_mut::<Model>().unwrap().pending_first_send = Some(pending);
        create_initial_conversation(world, false);
        return;
    };
    let mut request = crate::chat::turn_request(session, doc.text(), provider, model);
    if let Some(defaults) = &m.defaults {
        request.reasoning_effort = defaults.reasoning_effort.clone().unwrap_or_default();
        if let Some(matrix) = &defaults.stage_routing {
            request.stage_routing = crate::chat::align_saved_routes(matrix, provider, model);
        }
    }
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

fn apply_reply(world: &mut World, reply: Reply) {
    if !crate::workshops::receive(world, &reply) {
        conversations::reply(world, reply);
    }
}
pub(crate) fn workshop_unavailable(world: &mut World) {
    let m = world.resource_mut::<Model>().unwrap();
    if m.chat.session().is_none() && m.requested_session.is_none() {
        m.loading = false;
        m.chat.status = "Saved workshop unavailable. Retry in Settings → Connection.".into();
    }
}
/// Called only when an exact saved connection has advertised its authority.
pub(crate) fn workshop_ready(world: &mut World, owner: &str) {
    if crate::workshops::owner(world, None).as_deref() != Some(owner) {
        return;
    }
    let commands = crate::workshops::commands(world, owner);
    let (models, modes) = crate::workshops::catalog(world, owner);
    let defaults_pending = crate::workshops::chat_defaults(world, owner).is_none();
    let model = world.resource_mut::<Model>().unwrap();
    if model.owner.is_empty() && model.chat.session().is_none() {
        model.owner = owner.into();
        model.commands = commands;
        model.models = models;
        model.modes = modes;
        model.loading = false;
        model.defaults_pending = defaults_pending;
        model.chat.status = "Connected".into();
    }
    if let Some(defaults) = crate::workshops::chat_defaults(world, owner) {
        conversations::defaults(world, owner, defaults);
    }
}
pub(crate) fn workshop_defaults(
    world: &mut World,
    owner: &str,
    defaults: medousa_types::TuiDefaults,
) {
    conversations::defaults(world, owner, defaults);
}
pub(crate) fn workshop_catalog(
    world: &mut World,
    owner: &str,
    models: Vec<(String, String)>,
    modes: Vec<(String, String, bool)>,
    rows: Vec<(String, String)>,
    summaries: Vec<medousa_types::SessionHistorySummary>,
    cursor: Option<String>,
) {
    conversations::catalog(world, owner, models, modes, rows, cursor);
    chat_sidebar::receive(world, summaries, false);
}
pub(crate) fn rebind_workshop_view(world: &mut World, surface: crate::surfaces::SurfaceId) -> bool {
    crate::surfaces::rebind(world, surface)
}
fn apply_current_reply(world: &mut World, reply: Reply) {
    let m = world.resource::<Model>().unwrap();
    if reply.generation != m.generation
        && !matches!(
            reply.update,
            Update::Models(_) | Update::Modes(_) | Update::Unavailable(_)
        )
    {
        return;
    }
    if matches!(
        &reply.update,
        Update::History(..)
            | Update::SettledHistory(..)
            | Update::Admitted(..)
            | Update::Stream(..)
            | Update::Failure(..)
    ) {
        let m = world.resource_mut::<Model>().unwrap();
        m.transcript_revision = m.transcript_revision.wrapping_add(1);
    }
    match reply.update {
        Update::IdleTick => {}
        Update::IdleHistory {
            session,
            revision,
            snapshot,
        } => {
            let m = world.resource_mut::<Model>().unwrap();
            m.idle_refresh_pending = false;
            m.idle_refresh_delay = if snapshot.is_some() {
                std::time::Duration::from_secs(2)
            } else {
                (m.idle_refresh_delay * 2).min(std::time::Duration::from_secs(30))
            };
            if revision != m.transcript_revision
                || m.loading
                || m.chat.busy()
                || m.chat.uncertain
                || m.chat.session() != Some(session.as_str())
            {
                return;
            }
            if let Some((authority, turns, active)) = snapshot {
                let attach = active.clone();
                let attach_session = session.clone();
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
                if let Err(error) = m.chat.hydrate(session, authority, turns, active) {
                    m.chat.fail(error);
                }
                m.transcript_revision = m.transcript_revision.wrapping_add(1);
                if !m.chat.uncertain
                    && let Some(turn) = attach
                {
                    dispatch(world, Command::Attach(attach_session, turn));
                }
            }
        }
        Update::PeerStream {
            session,
            key,
            event,
        } => {
            collaboration::peers::stream_event(world, &session, &key, event);
        }
        Update::PeerCollaboration { session, result } => {
            collaboration::peers::receive(world, &session, result);
        }
        Update::ThreadReply {
            session,
            work_id,
            nonce,
            result,
        } => {
            collaboration::reply_received(world, &session, &work_id, nonce, result);
        }
        Update::Collaboration { session, result } => {
            collaboration::receive(world, &session, result);
        }
        Update::SessionsPage {
            query,
            rows,
            summaries,
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
            chat_sidebar::receive(world, summaries, append);
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
            let _ = m;
            conversations::open(world, Some(session));
        }
        Update::History(session, authority, turns, active) => {
            if let Err(e) = switch_document(world, &session) {
                world.resource_mut::<Model>().unwrap().chat.fail(e);
                return;
            }
            auxiliary::send(
                world,
                auxiliary::Command::Approval(approvals::refresh(&session)),
            );
            runtimes::refresh(world, &session);
            auxiliary::send(
                world,
                auxiliary::Command::Context(context::refresh(&session)),
            );
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
            let pending = m.pending_first_send.take();
            let files = std::mem::take(&mut m.pending_drop_files);
            let session = m.chat.session().map(str::to_owned);
            if let Some(session) = session {
                if let Err(error) = attachments::queue_files(world, &session, files) {
                    world.resource_mut::<Model>().unwrap().chat.status = error;
                }
            }
            if let Some((document, revision)) = pending {
                let unchanged = world
                    .resource::<SourceDocuments>()
                    .unwrap()
                    .get(document)
                    .is_some_and(|d| d.revision() == revision && !d.is_composing());
                if unchanged {
                    send_native(world);
                }
            }
        }
        Update::SettledHistory(session, authority, turns, settled_turn, active) => {
            let m = world.resource_mut::<Model>().unwrap();
            // A delayed reconciliation must not settle a newer attached turn.
            let current = m.chat.snapshot().and_then(|s| s.active_turn_id.as_deref());
            if current.is_some_and(|id| {
                id != settled_turn && active.as_ref().is_none_or(|next| next.turn_id != id)
            }) {
                return;
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
            if let Err(e) = m.chat.hydrate(session, authority, turns, active) {
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
            let settles_current = m
                .chat
                .snapshot()
                .and_then(|s| s.active_turn_id.as_deref())
                .is_none_or(|id| id == envelope.turn_id);
            if let Err(e) = m.chat.apply(envelope) {
                m.chat.fail(e);
            }
            if terminal && settles_current {
                m.chat.uncertain = true;
                m.chat.status = "Reconciling history…".into();
            }
        }
        Update::Defaults(defaults) => {
            apply_defaults(world.resource_mut::<Model>().unwrap(), *defaults);
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
            if let Some(defaults) = m.defaults.clone() {
                apply_defaults(m, defaults);
            }
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
            m.pending_first_send = None;
            m.pending_runtime_picker = false;
            m.pending_drop_files.clear();
        }
        Update::Unavailable(message) => {
            let m = world.resource_mut::<Model>().unwrap();
            m.pending_first_send = None;
            m.pending_runtime_picker = false;
            m.pending_drop_files.clear();
            m.chat.status = message;
            m.loading = false;
        }
    }
}
// Defaults are workshop-owned, while explicit choices and historical routes
// remain conversation-owned. Metadata can arrive before or after the catalog.
fn apply_defaults(m: &mut Model, defaults: medousa_types::TuiDefaults) {
    let preferred = defaults
        .provider
        .as_deref()
        .zip(defaults.model.as_deref())
        .map(|(p, n)| (p.trim().to_owned(), n.trim().to_owned()))
        .filter(|(p, n)| !p.is_empty() && !n.is_empty());
    if let Some(pair) = preferred {
        let i = m.models.iter().position(|v| *v == pair).unwrap_or_else(|| {
            m.models.push(pair);
            m.models.len() - 1
        });
        if !m.model_chosen {
            m.selected = Some(i);
        }
    }
    if !m.depth_chosen {
        m.depth = match defaults.response_depth_mode.as_deref() {
            Some("concise") => 0,
            Some("deep") => 2,
            _ => 1,
        };
    }
    m.defaults_pending = false;
    m.defaults = Some(defaults);
    restore_history_model(m);
}

// Resolve the last persisted response against the current catalog. Never
// silently display catalog entry zero when reopening a known conversation.
fn restore_history_model(m: &mut Model) {
    if m.model_chosen {
        return;
    }
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
    // The same retained chat can now live in a narrow split group. Fitted
    // greeting text must wrap inside that pane, not its former window width.
    if let Some(width) = world
        .get::<Rect>(ui.empty_presence)
        .map(|r| r.w)
        .filter(|w| *w > 0.)
    {
        for entity in children_of(world, ui.empty_presence) {
            if let Some(fit) = world.get_mut::<visual::FitLabel>(entity) {
                if fit.min == 0. && fit.label == entity {
                    fit.max = width.clamp(1., 360.);
                }
            }
        }
    }
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
    // A healthy connection needs no standing announcement; problems and
    // transient outcomes still appear in the footer.
    if m.chat.status == "Connected" {
        world.insert(ui.status, Hidden);
    } else {
        world.remove::<Hidden>(ui.status);
    }
    approvals::sync(world, ui.approvals, m.chat.session(), m.chat.snapshot());
    runtimes::show(world, m.chat.session());
    let chip = format!("{}  ⌄", runtimes::label(world, m.chat.session()));
    button(
        world,
        ui.controls,
        RUNTIME,
        &chip,
        !m.loading && !m.chat.busy() && !m.chat.uncertain,
    );
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
            auxiliary::send(
                world,
                auxiliary::Command::Approval(approvals::refresh(session)),
            );
            auxiliary::send(
                world,
                auxiliary::Command::Context(context::refresh(session)),
            );
        }
    }

    attachments::sync(world, ui.attachments, m.chat.session());
    let external = !matches!(runtimes::active(world), runtimes::ActiveRuntime::Native);
    let available = !m.loading
        && !m.chat.busy()
        && !runtimes::busy(world)
        && !world.resource::<Model>().unwrap().defaults_pending;
    let selector_key = format!(
        "{:?}{:?}{:?}{}{}{}",
        m.modes, m.selected, m.mode, m.depth, available, external
    );
    let state = world.resource_mut::<Model>().unwrap();
    let selectors_changed = state.selectors_key != selector_key;
    state.selectors_key = selector_key;
    if selectors_changed {
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
        props.enabled = available && !external;
        let selected = m
            .mode
            .as_ref()
            .and_then(|mode| m.modes.iter().position(|(id, _, _)| id == mode))
            .map(|i| OptionId(i as u64));
        SingleSelect::mount(world, ui.controls, UiId(11), props, &modes, selected).unwrap();
        // Match the conversation rows: selection is a quiet fill, not an
        // accent stripe. RNDR still owns focus, selected semantics and the
        // high-contrast shape cue. Reapply after catalog changes add rows.
        for popup in world.entities_with::<AnchoredPopup>() {
            if world.get::<AnchoredPopup>(popup).unwrap().anchor == ui.mode {
                for row in children_of(world, popup) {
                    if world.has::<SelectedState>(row) {
                        world.insert(row, controls::QuietSelection);
                    }
                }
            }
        }
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
        for entity in [ui.mode, ui.depth] {
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
    let send_enabled = available
        && !m.chat.uncertain
        && m.connected
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
        MODEL_PICKER,
        &format!("{model_title}  ⌄"),
        available && !external,
        260.,
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
        available && !external,
        160.,
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
        190.,
    );
    world.get_mut::<Label>(ui.model_caption).unwrap().text = m
        .selected
        .and_then(|i| m.models.get(i))
        .map(|(provider, _)| provider_caption(provider))
        .unwrap_or_default();
    let behavior_button = button(world, ui.controls, BEHAVIOR, "Behavior", true);
    let open = !world.has::<Hidden>(ui.behavior);
    world.insert(
        behavior_button.root,
        SelectionAccess {
            role: AccessRole::Button,
            checked: None,
            expanded: Some(open),
            multiselect: false,
            controls: open.then_some(ui.behavior),
            labelled_by: None,
            active_descendant: None,
        },
    );
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

    // The core is borrowed into m during presentation; world.Model.chat is empty.
    chat_sidebar::render(
        world,
        ui.sessions,
        &m.sessions,
        selected_session,
        m.chat
            .snapshot()
            .map(|s| s.authority_id.clone())
            .filter(|a| !a.is_empty()),
    );
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
    // Don't present an empty room while history is loading, or for an
    // external provider whose own conversation may not yet have arrived.
    // A connected new conversation without a session is also an empty room.
    let show_presence = provider.is_none()
        && !m.loading
        && m.chat.snapshot().map_or(m.connected, |snapshot| {
            snapshot.items.is_empty() && snapshot.active_turn_id.is_none()
        });
    let timeline_frame = world.get::<Parent>(ui.timeline).unwrap().entity;
    if show_presence {
        world.remove::<Hidden>(ui.empty_presence);
        world.remove::<Hidden>(ui.presence_tail);
        world.insert(timeline_frame, Hidden);
    } else {
        world.insert(ui.empty_presence, Hidden);
        world.insert(ui.presence_tail, Hidden);
        world.remove::<Hidden>(timeline_frame);
    }
    let state = world.resource::<Model>().unwrap();
    let (details, filter, sidebar) = (
        state.details_open,
        state.model_search_open,
        state.sidebar_open,
    );
    for (entity, visible) in [(ui.details, details), (ui.filter_row, filter)] {
        if visible {
            world.remove::<Hidden>(entity);
        } else {
            world.insert(entity, Hidden);
        }
    }
    let width = if crate::surfaces::active(world) == crate::surfaces::SurfaceId::Bots {
        crate::surfaces::chat_width(world)
    } else {
        workspace::sync(world, sidebar, details)
    };
    let width = world
        .get::<Rect>(ui.exchange.main)
        .map(|r| width.min(r.w))
        .unwrap_or(width);
    world.get_mut::<Layout>(ui.compose).unwrap().width = Size::Px(visual::column_width(width));
    collaboration::sync(world, ui, m.chat.snapshot());
    sync_composer_height(
        world,
        ui.composer,
        ui.composer_parent,
        world.resource::<Model>().unwrap().document,
    );
    world.resource_mut::<Model>().unwrap().chat = m.chat;
    model_picker::sync(world, available);
    if let Some(error) = workspace::error(world) {
        world.resource_mut::<Model>().unwrap().chat.status = error;
    }
}

#[cfg(test)]
#[path = "ui_visual_tests.rs"]
pub(crate) mod visual_tests;

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
    #[test]
    fn idle_refresh_is_read_only_single_flight_and_skips_uncertain_text() {
        let (mut app, mut rx) = idle_fixture();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.idle_refresh = std::time::Instant::now() - std::time::Duration::from_secs(3);
        refresh_idle_chat(app.world_mut());
        refresh_idle_chat(app.world_mut());
        assert!(matches!(
            rx.try_recv().unwrap().command,
            Command::Refresh(_, 0)
        ));
        assert!(rx.try_recv().is_err());
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.idle_refresh_pending = false;
        m.idle_refresh = std::time::Instant::now() - std::time::Duration::from_secs(3);
        m.chat.fail("retained fatal prose".into());
        refresh_idle_chat(app.world_mut());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn idle_history_cannot_overwrite_newer_transcript_or_admission() {
        for loading in [false, true] {
            let (mut app, _rx) = idle_fixture();
            let m = app.world_mut().resource_mut::<Model>().unwrap();
            let generation = m.generation;
            m.transcript_revision = if loading { 0 } else { 1 };
            m.loading = loading;
            apply_current_reply(app.world_mut(), Reply { generation, update: Update::IdleHistory {
                session: "s".into(), revision: 0,
                snapshot: Some(("a".into(), vec![], Some(serde_json::from_value(serde_json::json!({
                    "turn_id":"obsolete", "session_id":"s", "stream_url":"/v1/interactive/turn/obsolete/stream",
                    "phase":"running", "composer_handoff":false, "started_at":"2026-11-07T00:00:01Z"
                })).unwrap()))),
            }});
            assert!(!app.world().resource::<Model>().unwrap().chat.busy());
        }
    }

    #[test]
    fn idle_history_projects_durable_late_worker_receipt_without_active_turn() {
        let (mut app, _rx) = idle_fixture();
        let generation = app.world().resource::<Model>().unwrap().generation;
        let receipt = serde_json::json!({
            "entry_id":"receipt-entry", "entry_seq":1, "role":"assistant",
            "content":"Late worker receipt", "timestamp":"2026-11-07T00:00:01Z",
            "caused_by":{"authority_id":"a", "session_id":"s", "execution_id":"worker_synthesis"},
            "content_digest":"sha256:receipt", "tool_names":[]
        });
        apply_current_reply(
            app.world_mut(),
            Reply {
                generation,
                update: Update::IdleHistory {
                    session: "s".into(),
                    revision: 0,
                    snapshot: Some(("a".into(), vec![receipt], None)),
                },
            },
        );
        let chat = &app.world().resource::<Model>().unwrap().chat;
        assert!(!chat.busy());
        assert!(!chat.uncertain);
        assert!(
            chat.snapshot()
                .unwrap()
                .items
                .iter()
                .any(|i| i.content == "Late worker receipt")
        );
    }

    #[test]
    fn idle_history_attaches_callback_without_reopening_conversation() {
        let (mut app, _rx) = idle_fixture();
        let generation = app.world().resource::<Model>().unwrap().generation;
        apply_current_reply(app.world_mut(), Reply { generation, update: Update::IdleHistory {
            session: "s".into(), revision: 0,
            snapshot: Some(("a".into(), vec![], Some(serde_json::from_value(serde_json::json!({
                "turn_id":"callback", "session_id":"s", "stream_url":"/v1/interactive/turn/callback/stream",
                "phase":"running", "composer_handoff":false, "started_at":"2026-11-07T00:00:01Z"
            })).unwrap()))),
        }});
        let m = app.world().resource::<Model>().unwrap();
        assert_eq!(
            m.chat.snapshot().unwrap().active_turn_id.as_deref(),
            Some("callback")
        );
        assert!(!m.chat.uncertain);
    }

    #[test]
    fn late_terminal_history_does_not_settle_newer_attached_turn() {
        let (mut app, _rx) = idle_fixture();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.chat
            .admitted("newer".into(), "new prompt".into())
            .unwrap();
        let generation = m.generation;
        apply_current_reply(
            app.world_mut(),
            Reply {
                generation,
                update: Update::SettledHistory("s".into(), "a".into(), vec![], "old".into(), None),
            },
        );
        let chat = &app.world().resource::<Model>().unwrap().chat;
        assert_eq!(
            chat.snapshot().unwrap().active_turn_id.as_deref(),
            Some("newer")
        );
        assert!(!chat.uncertain);
        assert!(
            chat.snapshot()
                .unwrap()
                .items
                .iter()
                .any(|i| i.content == "new prompt")
        );
    }
    #[test]
    fn terminal_history_attaches_newer_callback_and_retains_live_prose() {
        let (mut app, _rx) = idle_fixture();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.chat.admitted("old".into(), "prompt".into()).unwrap();
        m.chat
            .apply(
                serde_json::from_value(serde_json::json!({
                    "schema_version":3, "turn_id":"old", "seq":1,
                    "emitted_at_utc":"2026-11-07T00:00:00Z",
                    "event":{"type":"content_append", "segment_id":"a", "text":"live prose"}
                }))
                .unwrap(),
            )
            .unwrap();
        let generation = m.generation;
        let next = serde_json::from_value(serde_json::json!({
            "turn_id":"next", "session_id":"s", "stream_url":"/v1/interactive/turn/next/stream",
            "phase":"running", "composer_handoff":false, "started_at":"2026-11-07T00:00:01Z"
        }))
        .unwrap();
        apply_current_reply(
            app.world_mut(),
            Reply {
                generation,
                update: Update::SettledHistory(
                    "s".into(),
                    "a".into(),
                    vec![],
                    "old".into(),
                    Some(next),
                ),
            },
        );
        let chat = &app.world().resource::<Model>().unwrap().chat;
        assert_eq!(
            chat.snapshot().unwrap().active_turn_id.as_deref(),
            Some("next")
        );
        assert!(!chat.uncertain);
        assert!(
            chat.snapshot()
                .unwrap()
                .items
                .iter()
                .any(|i| i.content.contains("live prose"))
        );
    }
    #[test]
    fn empty_presence_only_appears_for_loaded_empty_chat() {
        let (mut app, _rx) = idle_fixture();
        let ui = *app.world().resource::<Ui>().unwrap();
        let frame = app.world().get::<Parent>(ui.timeline).unwrap().entity;
        assert!(!app.world().has::<Hidden>(ui.empty_presence));
        assert!(app.world().has::<Hidden>(frame));
        app.world_mut().resource_mut::<Model>().unwrap().loading = true;
        app.update();
        assert!(app.world().has::<Hidden>(ui.empty_presence));
        assert!(!app.world().has::<Hidden>(frame));
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.loading = false;
        m.chat.hydrate("other".into(), "a".into(), vec![
            serde_json::json!({"entry_id":"u", "entry_seq":1, "role":"user", "content":"Hello"})
        ], None).unwrap();
        app.update();
        assert!(app.world().has::<Hidden>(ui.empty_presence));
        assert!(!app.world().has::<Hidden>(frame));
    }
    #[test]
    fn sustained_composer_typing_acknowledges_source_events() {
        let (mut app, _rx) = idle_fixture();
        let document = app.world().resource::<Model>().unwrap().document;
        for _ in 0..300 {
            let d = app
                .world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(document)
                .unwrap();
            let end = d.text().len();
            d.commit(rndr_core::editing::SourceBatch {
                document,
                base: d.revision(),
                origin: rndr_core::editing::EditOrigin::Host(99),
                changes: vec![rndr_core::editing::SourceChange {
                    range: end..end,
                    replacement: "x".into(),
                }],
            })
            .expect("typing must not hit event backpressure");
            update(app.world_mut());
        }
        assert_eq!(
            app.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(document)
                .unwrap()
                .text(),
            "x".repeat(300)
        );
    }
    /// Reproducible CPU-side frame benchmark; excludes GPU/presentation and daemon IO.
    #[test]
    #[ignore = "explicit performance measurement"]
    fn native_frame_performance_probe() {
        let (mut app, _rx) = idle_fixture();
        for surface in [
            crate::surfaces::SurfaceId::Chat,
            crate::surfaces::SurfaceId::Notes,
            crate::surfaces::SurfaceId::Code,
        ] {
            crate::surfaces::select(app.world_mut(), surface);
            for _ in 0..12 {
                rndr_paint::frame(&mut app);
            }
            let mut samples = Vec::new();
            for _ in 0..80 {
                let start = std::time::Instant::now();
                rndr_paint::frame(&mut app);
                samples.push(start.elapsed().as_secs_f64() * 1000.);
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "native CPU frame {surface:?}: median={:.3}ms p95={:.3}ms",
                samples[40], samples[76]
            );
        }
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
    fn composer_text(app: &App) -> String {
        let m = app.world().resource::<Model>().unwrap();
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(m.document)
            .unwrap()
            .text()
            .into()
    }
    fn press(app: &mut App, key: NamedKey, modifiers: Modifiers) {
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(key, modifiers)];
        app.update();
    }
    fn preferred_defaults() -> medousa_types::TuiDefaults {
        medousa_types::TuiDefaults {
            provider: Some("preferred".into()),
            model: Some("saved-model".into()),
            response_depth_mode: Some("deep".into()),
            reasoning_effort: Some("high".into()),
            ..Default::default()
        }
    }
    fn initial_fixture() -> (App, tokio::sync::mpsc::UnboundedReceiver<Request>) {
        let mut app = fixture();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx);
        apply_defaults(
            app.world_mut().resource_mut::<Model>().unwrap(),
            preferred_defaults(),
        );
        app.update();
        (app, rx)
    }
    #[test]
    fn file_drop_event_is_consumed_once_by_update() {
        let (mut app, mut rx) = initial_fixture();
        while rx.try_recv().is_ok() {}
        let mut hover = FileHoverSet::default();
        hover.hover(FileHover::from_report("drop.txt", None));
        app.world_mut().insert_resource(hover);
        rndr_core::send(app.world_mut(), FileDrop::from_report("drop.txt", None));
        app.update();
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
        assert!(rx.try_recv().is_err());
        app.world_mut()
            .resource_mut::<FileHoverSet>()
            .unwrap()
            .cancel();
        app.update();
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn file_drop_initial_chat_promotes_without_sending_or_losing_draft() {
        let (mut app, mut rx) = initial_fixture();
        draft(&mut app, "Keep my draft");
        while rx.try_recv().is_ok() {}
        let document = app.world().resource::<Model>().unwrap().document;
        admit_composer_files(app.world_mut(), vec!["/device/drop.txt".into()]);
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
        assert!(rx.try_recv().is_err());
        assert_eq!(composer_text(&app), "Keep my draft");
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::History("created".into(), "owner".into(), vec![], None),
            },
        );
        let uploads = attachments::drain_commands(app.world_mut());
        assert_eq!(uploads.len(), 1);
        assert_eq!(uploads[0].session, "created");
        assert!(
            attachments::media_refs(app.world(), "created")
                .unwrap_err()
                .contains("Wait")
        );
        assert_eq!(app.world().resource::<Model>().unwrap().document, document);
        assert_eq!(composer_text(&app), "Keep my draft");
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn file_drop_creation_failure_clears_paths_and_never_auto_retries() {
        let (mut app, mut rx) = initial_fixture();
        while rx.try_recv().is_ok() {}
        admit_composer_files(app.world_mut(), vec!["drop.txt".into()]);
        rx.try_recv().unwrap();
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::Failure("offline".into()),
            },
        );
        assert!(
            app.world()
                .resource::<Model>()
                .unwrap()
                .pending_drop_files
                .is_empty()
        );
        assert!(attachments::drain_commands(app.world_mut()).is_empty());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn file_drop_target_respects_composer_bounds_active_surface_and_path_only_reports() {
        let (mut app, _) = initial_fixture();
        let ui = *app.world().resource::<Ui>().unwrap();
        app.world_mut().insert(
            ui.compose,
            Rect {
                x: 100.,
                y: 200.,
                w: 300.,
                h: 80.,
            },
        );
        assert_eq!(
            conversations::drop_target(app.world(), Some([150., 220.])),
            Some(ui.compose)
        );
        assert_eq!(
            conversations::drop_target(app.world(), Some([50., 220.])),
            None
        );
        assert_eq!(
            conversations::drop_target(app.world(), Some([f32::NAN, 220.])),
            None
        );
        assert_eq!(
            conversations::drop_target(app.world(), None),
            Some(ui.compose)
        );
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
        assert_eq!(conversations::drop_target(app.world(), None), None);
    }
    #[test]
    fn file_drop_over_limit_does_not_create_conversation() {
        let (mut app, mut rx) = initial_fixture();
        while rx.try_recv().is_ok() {}
        admit_composer_files(
            app.world_mut(),
            (0..11).map(|i| format!("{i}.txt").into()).collect(),
        );
        assert!(rx.try_recv().is_err());
        assert!(
            app.world()
                .resource::<Model>()
                .unwrap()
                .pending_drop_files
                .is_empty()
        );
    }
    fn mode_catalog(app: &mut App) {
        app.world_mut().resource_mut::<Model>().unwrap().modes = vec![
            ("assistant".into(), "Assistant".into(), true),
            ("teacher".into(), "Teacher".into(), true),
            ("coder".into(), "Coder".into(), false),
        ];
        app.update();
    }
    #[test]
    fn mode_selector_opens_in_initial_view_and_persists_draft_choice() {
        let (mut app, mut rx) = initial_fixture();
        mode_catalog(&mut app);
        let mode = app.world().resource::<Ui>().unwrap().mode;
        assert_eq!(
            app.world().resource::<Model>().unwrap().mode.as_deref(),
            Some("assistant")
        );
        assert!(app.world().get::<Button>(mode).unwrap().enabled);
        request_focus(app.world_mut(), mode);
        press(&mut app, NamedKey::Enter, Modifiers::default());
        let popup = app
            .world()
            .entities_with::<AnchoredPopup>()
            .into_iter()
            .find(|&e| app.world().get::<AnchoredPopup>(e).unwrap().anchor == mode)
            .unwrap();
        assert!(
            !app.world().has::<Hidden>(popup),
            "mode popup must actually open"
        );
        let rows = children_of(app.world(), popup)
            .into_iter()
            .filter(|&row| app.world().has::<SelectedState>(row))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        for &row in &rows {
            assert!(app.world().has::<controls::QuietSelection>(row));
            // Selection's separate, absolutely positioned edge marker is
            // hidden; the selected row and accessible state are retained.
            for child in children_of(app.world(), row) {
                if app.world().has::<Absolute>(child) {
                    assert!(app.world().has::<Hidden>(child));
                }
            }
        }
        assert!(app.world().get::<SelectedState>(rows[0]).unwrap().0);
        press(&mut app, NamedKey::ArrowDown, Modifiers::default());
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert_eq!(
            app.world().resource::<Model>().unwrap().mode.as_deref(),
            Some("teacher")
        );
        // An authoritative Coder mode/catalog refresh uses the same quiet
        // selection treatment, including rows newly added by the catalog.
        {
            let state = app.world_mut().resource_mut::<Model>().unwrap();
            state.modes[2].2 = true;
            state.mode = Some("coder".into());
            state.modes.push(("general".into(), "General".into(), true));
        }
        app.update();
        let refreshed = children_of(app.world(), popup)
            .into_iter()
            .filter(|&row| app.world().has::<SelectedState>(row))
            .collect::<Vec<_>>();
        assert_eq!(refreshed.len(), 4);
        assert!(app.world().get::<SelectedState>(refreshed[2]).unwrap().0);
        for row in refreshed {
            assert!(app.world().has::<controls::QuietSelection>(row));
            for child in children_of(app.world(), row) {
                if app.world().has::<Absolute>(child) {
                    assert!(app.world().has::<Hidden>(child));
                }
            }
        }
        app.world_mut().resource_mut::<Model>().unwrap().mode = Some("teacher".into());
        app.update();
        while rx.try_recv().is_ok() {}
        draft(&mut app, "Teach me");
        action(app.world_mut(), SEND);
        assert!(matches!(
            rx.try_recv().unwrap().command,
            Command::NewWithMode(medousa_types::AgentModeId::Teacher)
        ));
    }
    #[test]
    fn existing_chat_mode_change_waits_for_server_effective_mode() {
        let (mut app, mut rx) = initial_fixture();
        mode_catalog(&mut app);
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::History("s".into(), "owner".into(), vec![], None),
            },
        );
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::Mode("general".into()),
            },
        );
        app.update();
        let mode = app.world().resource::<Ui>().unwrap().mode;
        while rx.try_recv().is_ok() {}
        rndr_core::send(
            app.world_mut(),
            SelectionChange {
                control: mode,
                value: SelectionValue::Selected(vec![OptionId(1)]),
            },
        );
        app.update();
        assert!(
            matches!(rx.try_recv().unwrap().command, Command::SetMode(ref s, medousa_types::AgentModeId::Teacher) if s == "s")
        );
        assert_eq!(
            app.world().resource::<Model>().unwrap().mode.as_deref(),
            Some("general")
        );
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::Mode("teacher".into()),
            },
        );
        assert_eq!(
            app.world().resource::<Model>().unwrap().mode.as_deref(),
            Some("teacher")
        );
    }
    #[test]
    fn initial_runtime_picker_promotes_same_view_without_sending_draft() {
        let (mut app, mut rx) = initial_fixture();
        draft(&mut app, "Do not send yet");
        let ui = *app.world().resource::<Ui>().unwrap();
        let document = app.world().resource::<Model>().unwrap().document;
        app.world_mut().resource_mut::<Model>().unwrap().mode = Some("teacher".into());
        // Runtime selection must not require a native provider/model.
        app.world_mut().resource_mut::<Model>().unwrap().selected = None;
        while rx.try_recv().is_ok() {}
        let trigger = button(app.world_mut(), ui.controls, RUNTIME, "Medousa  ⌄", true);
        assert_eq!(
            try_activate(app.world_mut(), trigger.root),
            ActivationResult::Accepted
        );
        app.update();
        assert!(matches!(
            rx.try_recv().unwrap().command,
            Command::NewWithMode(medousa_types::AgentModeId::Teacher)
        ));
        action(app.world_mut(), RUNTIME);
        action(app.world_mut(), SEND);
        assert!(
            rx.try_recv().is_err(),
            "no duplicate creation or send while pending"
        );
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::History("created".into(), "owner".into(), vec![], None),
            },
        );
        app.update();
        while let Ok(request) = rx.try_recv() {
            assert!(
                !matches!(
                    request.command,
                    Command::Send(_) | Command::New | Command::NewWithMode(_)
                ),
                "opening runtime menu emitted a mutation"
            );
        }
        assert_eq!(composer_text(&app), "Do not send yet");
        assert_eq!(app.world().resource::<Model>().unwrap().document, document);
        assert_eq!(
            app.world().resource::<Ui>().unwrap().composer.editor,
            ui.composer.editor
        );
        assert_eq!(
            app.world().resource::<Model>().unwrap().chat.session(),
            Some("created")
        );
        assert!(
            app.world()
                .query::<Label>()
                .iter()
                .any(|(_, l)| l.text == "Refresh runtimes")
        );
    }
    #[test]
    fn initial_runtime_creation_failure_keeps_draft_without_automatic_replay() {
        let (mut app, mut rx) = initial_fixture();
        draft(&mut app, "Keep this");
        while rx.try_recv().is_ok() {}
        action(app.world_mut(), RUNTIME);
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::Failure("offline".into()),
            },
        );
        assert!(
            !app.world()
                .resource::<Model>()
                .unwrap()
                .pending_runtime_picker
        );
        assert_eq!(composer_text(&app), "Keep this");
        // A failed creation may have been admitted: never replay it on a click.
        action(app.world_mut(), RUNTIME);
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn first_send_button_waits_for_defaults_then_creates_in_visible_composer() {
        let (mut app, mut rx) = initial_fixture();
        draft(&mut app, "First click");
        while rx.try_recv().is_ok() {}
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .defaults_pending = true;
        app.update();
        let ui = *app.world().resource::<Ui>().unwrap();
        let send = button(app.world_mut(), ui.controls, SEND, "↑", false);
        app.update();
        assert!(!app.world().get::<Button>(send.root).unwrap().enabled);
        action(app.world_mut(), SEND);
        assert!(rx.try_recv().is_err());
        apply_defaults(
            app.world_mut().resource_mut::<Model>().unwrap(),
            preferred_defaults(),
        );
        app.update();
        let mut at = Some(ui.composer.editor);
        while let Some(e) = at {
            assert!(
                !app.world().has::<Hidden>(e),
                "initial editor hidden at {e:?}"
            );
            at = app.world().get::<Parent>(e).map(|p| p.entity);
        }
        assert!(app.world().get::<Button>(send.root).unwrap().enabled);
        assert_eq!(
            try_activate(app.world_mut(), send.root),
            ActivationResult::Accepted
        );
        app.update();
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
    }
    #[test]
    fn first_enter_promotes_initial_composer_in_place_and_sends_once() {
        let (mut app, mut rx) = initial_fixture();
        draft(&mut app, "First message");
        while rx.try_recv().is_ok() {}
        let ui = *app.world().resource::<Ui>().unwrap();
        let document = app.world().resource::<Model>().unwrap().document;
        request_focus(app.world_mut(), ui.composer.editor);
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert!(rx.try_recv().is_err());
        assert_eq!(composer_text(&app), "First message");
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::History("created".into(), "owner".into(), vec![], None),
            },
        );
        let request = rx.try_recv().unwrap();
        let Command::Send(request) = request.command else {
            panic!("expected first send")
        };
        assert_eq!(request.session_id, "created");
        assert_eq!(request.prompt, "First message");
        assert_eq!(
            (&*request.provider, &*request.model),
            ("preferred", "saved-model")
        );
        assert_eq!(request.response_depth_mode, "deep");
        assert_eq!(request.reasoning_effort, "high");
        assert!(rx.try_recv().is_err());
        assert_eq!(app.world().resource::<Model>().unwrap().document, document);
        assert_eq!(
            app.world().resource::<Ui>().unwrap().composer.editor,
            ui.composer.editor
        );
        assert_eq!(app.world().resource::<Model>().unwrap().generation, 0);
        assert_eq!(
            app.world().resource::<Model>().unwrap().chat.session(),
            Some("created")
        );
        assert_eq!(composer_text(&app), "First message"); // Clear only on admission.
        app.update();
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(ui.composer.editor)
        );
    }
    #[test]
    fn failed_initial_creation_keeps_draft_and_does_not_auto_retry() {
        let (mut app, mut rx) = initial_fixture();
        draft(&mut app, "Keep me");
        while rx.try_recv().is_ok() {}
        action(app.world_mut(), SEND);
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::Failure("Unavailable".into()),
            },
        );
        app.update();
        assert_eq!(composer_text(&app), "Keep me");
        let m = app.world().resource::<Model>().unwrap();
        assert!(!m.loading);
        assert!(m.pending_first_send.is_none());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn editing_initial_draft_during_creation_never_sends_unapproved_text() {
        let (mut app, mut rx) = initial_fixture();
        draft(&mut app, "Original");
        while rx.try_recv().is_ok() {}
        action(app.world_mut(), SEND);
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
        draft(&mut app, "Edited while creating");
        apply_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::History("created".into(), "a".into(), vec![], None),
            },
        );
        assert!(rx.try_recv().is_err());
        assert_eq!(composer_text(&app), "Edited while creating");
        assert!(!app.world().resource::<Model>().unwrap().loading);
    }
    #[test]
    fn defaults_and_catalog_arrival_order_preserve_preference_and_explicit_choices() {
        for defaults_first in [false, true] {
            let mut app = fixture();
            let catalog = vec![("first".into(), "not-preferred".into())];
            if defaults_first {
                apply_current_reply(
                    app.world_mut(),
                    Reply {
                        generation: 0,
                        update: Update::Defaults(Box::new(preferred_defaults())),
                    },
                );
            }
            apply_current_reply(
                app.world_mut(),
                Reply {
                    generation: 0,
                    update: Update::Models(catalog.clone()),
                },
            );
            if !defaults_first {
                apply_current_reply(
                    app.world_mut(),
                    Reply {
                        generation: 0,
                        update: Update::Defaults(Box::new(preferred_defaults())),
                    },
                );
            }
            let m = app.world_mut().resource_mut::<Model>().unwrap();
            assert_eq!(
                m.models[m.selected.unwrap()],
                ("preferred".into(), "saved-model".into())
            );
            m.selected = Some(0);
            m.model_chosen = true;
            m.depth = 0;
            m.depth_chosen = true;
            apply_defaults(m, preferred_defaults());
            assert_eq!(m.selected, Some(0));
            assert_eq!(m.depth, 0);
        }
    }
    #[test]
    fn initial_blank_or_composing_draft_does_not_create_session() {
        let (mut app, mut rx) = initial_fixture();
        while rx.try_recv().is_ok() {}
        action(app.world_mut(), SEND);
        assert!(rx.try_recv().is_err());
        draft(&mut app, "Composing");
        let editor = app.world().resource::<Ui>().unwrap().composer.editor;
        request_focus(app.world_mut(), editor);
        app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "候補".into();
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn composer_enter_sends_without_appending_a_newline() {
        let (mut app, mut rx) = idle_fixture();
        draft(&mut app, "Send this exactly");
        while rx.try_recv().is_ok() {}
        let editor = app.world().resource::<Ui>().unwrap().composer.editor;
        request_focus(app.world_mut(), editor);
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert_eq!(composer_text(&app), "Send this exactly");
        assert!(matches!(rx.try_recv().unwrap().command, Command::Send(_)));
        assert!(rx.try_recv().is_err());
        // Admission has not yet acknowledged this draft: another Enter cannot duplicate it.
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn composer_primary_enter_still_sends() {
        let (mut app, mut rx) = idle_fixture();
        draft(&mut app, "Alternate send");
        while rx.try_recv().is_ok() {}
        let editor = app.world().resource::<Ui>().unwrap().composer.editor;
        request_focus(app.world_mut(), editor);
        let modifiers = if cfg!(target_os = "macos") {
            Modifiers {
                super_key: true,
                ..Default::default()
            }
        } else {
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
        };
        press(&mut app, NamedKey::Enter, modifiers);
        assert!(matches!(rx.try_recv().unwrap().command, Command::Send(_)));
        assert_eq!(composer_text(&app), "Alternate send");
    }
    #[test]
    fn composer_shift_enter_inserts_newline_without_sending() {
        let (mut app, mut rx) = idle_fixture();
        draft(&mut app, "Keep writing");
        while rx.try_recv().is_ok() {}
        let editor = app.world().resource::<Ui>().unwrap().composer.editor;
        request_focus(app.world_mut(), editor);
        press(
            &mut app,
            NamedKey::Enter,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        let text = composer_text(&app);
        assert_eq!(text.matches('\n').count(), 1, "{text:?}");
        assert!(text.contains("Keep writing"));
        assert!(rx.try_recv().is_err());
        assert!(!app.world().resource::<Model>().unwrap().loading);
    }
    #[test]
    fn composer_enter_does_not_send_empty_or_ime_input_or_search_fields() {
        let (mut app, mut rx) = idle_fixture();
        let ui = *app.world().resource::<Ui>().unwrap();
        while rx.try_recv().is_ok() {}
        request_focus(app.world_mut(), ui.composer.editor);
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert!(rx.try_recv().is_err());
        draft(&mut app, "Unsent draft");
        request_focus(app.world_mut(), ui.composer.editor);
        app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "候補".into();
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert!(!app.world().resource::<Model>().unwrap().loading);
        assert!(rx.try_recv().is_err());
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .ime_preedit
            .clear();
        // The search row shares the composer card, but not its keymap context.
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .model_search_open = true;
        app.update();
        request_focus(app.world_mut(), ui.model_filter);
        press(&mut app, NamedKey::Enter, Modifiers::default());
        assert!(rx.try_recv().is_err());
        assert!(!app.world().resource::<Model>().unwrap().loading);
    }
    #[test]
    fn nonchat_surfaces_cannot_dispatch_chat_shortcuts_or_hidden_send_actions() {
        for surface in [
            crate::surfaces::SurfaceId::Notes,
            crate::surfaces::SurfaceId::Code,
        ] {
            let (mut app, mut rx) = idle_fixture();
            draft(&mut app, "do not send this while editing Notes");
            while rx.try_recv().is_ok() {}
            let ui = *app.world().resource::<Ui>().unwrap();
            crate::surfaces::select(app.world_mut(), surface);
            app.world_mut().insert_resource(KeymapFired {
                actions: vec!["chat.send".into()],
            });
            // A stale keymap event must not cross the active surface boundary.
            update(app.world_mut());
            assert!(!app.world().resource::<Model>().unwrap().chat.busy());
            assert!(rx.try_recv().is_err());
            assert_eq!(
                app.world()
                    .resource::<SourceDocuments>()
                    .unwrap()
                    .get(app.world().resource::<Model>().unwrap().document)
                    .unwrap()
                    .text(),
                "do not send this while editing Notes"
            );
            assert_eq!(
                try_activate(app.world_mut(), ui.composer.editor),
                ActivationResult::Rejected
            );
            assert!(app.world().resource::<Focus>().unwrap().entity.is_none());
        }
    }
    #[test]
    fn composer_keeps_every_control_on_one_row_inside_the_card() {
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
            let row = w.get::<Rect>(ui.controls).unwrap();
            // Mode and model share the row under the editor, inside the card.
            assert!(
                mode.y >= editor.y + editor.h && (mode.y - model.y).abs() < 4.,
                "editor={editor:?}, model={model:?}, mode={mode:?}"
            );
            for rect in [mode, model] {
                assert!(
                    rect.x >= row.x && rect.x + rect.w <= row.x + row.w + 1.,
                    "{rect:?} escapes {row:?}"
                );
            }
            assert!(mode.x < model.x, "agent controls lead, model trails");
            assert!(
                editor.x + editor.w <= width && editor.w > 350.,
                "{editor:?}"
            );
            assert!(w.has::<ResolvedColor>(ui.composer.editor));
            assert_eq!(
                w.get::<Color>(ui.composer.editor),
                Some(&visual::color(w, composer_color()))
            );
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
        // Search hands focus back to the composer area rather than leaving
        // an unrelated modal scope open above the filter.
        assert!(w.has::<Hidden>(ui.behavior));
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
        let provider = model_picker::handle(app.world())
            .unwrap()
            .item(app.world(), 1)
            .unwrap();
        assert_eq!(
            try_activate(app.world_mut(), provider),
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
    fn history_and_defaults_refresh_never_replace_an_explicit_model() {
        let mut app = fixture();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.chat.hydrate("s".into(), "a".into(), vec![serde_json::json!({
            "role":"assistant", "content":"Hello", "parts":[{"kind":"model_receipt","provider":"history","model":"old"}]
        })], None).unwrap();
        m.models = vec![
            ("history".into(), "old".into()),
            ("chosen".into(), "new".into()),
        ];
        m.selected = Some(1);
        m.model_chosen = true;
        apply_defaults(m, preferred_defaults());
        assert_eq!(
            m.models[m.selected.unwrap()],
            ("chosen".into(), "new".into())
        );
        apply_current_reply(
            app.world_mut(),
            Reply {
                generation: 0,
                update: Update::Models(vec![
                    ("history".into(), "old".into()),
                    ("chosen".into(), "new".into()),
                ]),
            },
        );
        let m = app.world().resource::<Model>().unwrap();
        assert_eq!(
            m.models[m.selected.unwrap()],
            ("chosen".into(), "new".into())
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
    fn workspace_keyboard_resize_collapse_and_narrow_dock_keep_the_draft() {
        let (mut app, _rx) = idle_fixture();
        draft(&mut app, "Keep this unsent draft");
        for _ in 0..5 {
            app.update();
        }
        let ui = *app.world().resource::<Ui>().unwrap();
        let document = app.world().resource::<Model>().unwrap().document;
        let sash = app
            .world()
            .entities_with::<Semantics>()
            .into_iter()
            .find(|e| {
                app.world().get::<Semantics>(*e).unwrap().name.as_deref()
                    == Some("Conversation sidebar width")
            })
            .unwrap();
        let before = app.world().get::<Rect>(ui.sidebar).unwrap().w;
        request_focus(app.world_mut(), sash);
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::ArrowRight, Modifiers::default())];
        app.update();
        for _ in 0..4 {
            app.update();
        }
        let after = app.world().get::<Rect>(ui.sidebar).unwrap().w;
        assert!(after > before + 5., "{before} -> {after}");
        let ratio = app
            .world()
            .resource::<workspace::Workspace>()
            .unwrap()
            .prefs
            .sidebar_ratio;
        action(app.world_mut(), SIDEBAR);
        for _ in 0..5 {
            app.update();
        }
        assert!(
            app.world().get::<Rect>(ui.compose).unwrap().x < 260. + crate::surfaces::RAIL_WIDTH
        );
        action(app.world_mut(), SIDEBAR);
        for _ in 0..5 {
            app.update();
        }
        assert!((app.world().get::<Rect>(ui.sidebar).unwrap().w - after).abs() < 2.);
        action(app.world_mut(), DETAILS);
        app.world_mut().resource_mut::<Viewport>().unwrap().width = 760.;
        for _ in 0..8 {
            app.update();
        }
        let editor = app.world().get::<Rect>(ui.composer.editor).unwrap();
        let dock = app.world().get::<Rect>(ui.details).unwrap();
        assert!(
            editor.w >= 280. && editor.x + editor.w <= dock.x,
            "editor {editor:?}, dock {dock:?}"
        );
        assert!(dock.w >= 280. && dock.x + dock.w <= 761., "{dock:?}");
        assert_eq!(app.world().resource::<Model>().unwrap().document, document);
        assert_eq!(
            app.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(document)
                .unwrap()
                .text(),
            "Keep this unsent draft"
        );
        assert_eq!(
            app.world()
                .resource::<workspace::Workspace>()
                .unwrap()
                .prefs
                .sidebar_ratio,
            ratio
        );
        action(app.world_mut(), DETAILS);
        app.world_mut().resource_mut::<Viewport>().unwrap().width = 1200.;
        for _ in 0..8 {
            app.update();
        }
        assert!((app.world().get::<Rect>(ui.sidebar).unwrap().w - after).abs() < 2.);
    }
    #[test]
    fn workspace_continuous_pointer_drag_tracks_every_delta() {
        let (mut app, _rx) = idle_fixture();
        for _ in 0..8 {
            app.update();
        }
        let ui = *app.world().resource::<Ui>().unwrap();
        let before = app.world().get::<Rect>(ui.sidebar).unwrap().w;
        let sash = app
            .world()
            .entities_with::<Semantics>()
            .into_iter()
            .find(|e| {
                app.world().get::<Semantics>(*e).unwrap().name.as_deref()
                    == Some("Conversation sidebar width")
            })
            .unwrap();
        let rect = *app.world().get::<Rect>(sash).unwrap();
        let at = [rect.x + rect.w / 2., rect.y + rect.h / 2.];
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = at;
        input.pointer.press_at = Some(at);
        input.pointer.pressed = true;
        input.pointer.down = true;
        input.pointer.button = PointerButton::Left;
        app.update();
        for i in 1..=10 {
            let input = app.world_mut().resource_mut::<Input>().unwrap();
            input.pointer.position = [at[0] + i as f32 * 5., at[1]];
            input.pointer.press_at = None;
            input.pointer.pressed = false;
            input.pointer.down = true;
            app.update();
        }
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .pointer
            .down = false;
        app.update();
        let after = app.world().get::<Rect>(ui.sidebar).unwrap().w;
        assert!((after - before - 50.).abs() < 2., "{before} -> {after}");
    }
    #[test]
    fn workspace_details_dock_fits_beside_the_composer() {
        let (mut app, _rx) = idle_fixture();
        action(app.world_mut(), DETAILS);
        for _ in 0..8 {
            app.update();
        }
        let ui = *app.world().resource::<Ui>().unwrap();
        let dock = app.world().get::<Rect>(ui.details).unwrap();
        assert!(
            dock.w >= 280. && dock.h > 700. && dock.x + dock.w <= 1201.,
            "{dock:?}"
        );
        let compose = app.world().get::<Rect>(ui.compose).unwrap();
        assert!(
            compose.x + compose.w <= dock.x - 12.,
            "composer {compose:?}, dock {dock:?}"
        );
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

/// The models the engine offers, as `(provider, model)`.
pub(crate) fn models(world: &World) -> Vec<(String, String)> {
    world
        .resource::<Model>()
        .map(|m| m.models.clone())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn set_models(world: &mut World, models: Vec<(String, String)>) {
    world.resource_mut::<Model>().unwrap().models = models;
}

/// Shows a conversation in its owning view, without duplicating chat state.
pub(crate) fn open_conversation(world: &mut World, session: String) {
    if crate::bots::is_bot_session(world, &session) {
        open_bot_conversation(world, session);
    } else {
        crate::surfaces::select(world, crate::surfaces::SurfaceId::Chat);
        if crate::surfaces::active(world) == crate::surfaces::SurfaceId::Chat {
            conversations::open(world, Some(session));
        }
    }
}
pub(crate) fn open_bot_conversation(world: &mut World, session: String) {
    conversations::open_bot(world, session);
}
pub(crate) fn remember_bot_session(world: &mut World, session: String) {
    conversations::remember_bot_session(world, session);
}
pub(crate) fn is_known_bot_session(world: &World, session: &str) -> bool {
    conversations::known_bot_session(world, session)
}
pub(crate) fn activate_normal_chat(world: &mut World) {
    conversations::activate_normal(world);
}

pub fn external_admitted(world: &mut World, session: &str) {
    conversations::with_session(world, session, |world| {
        external_admitted_current(world, session)
    });
}
fn external_admitted_current(world: &mut World, session: &str) {
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
    conversations::with_session(world, session, |world| {
        runtime_event_current(world, session, command)
    });
}
fn runtime_event_current(world: &mut World, session: &str, command: medousa_app_core::ChatCommand) {
    let m = world.resource_mut::<Model>().unwrap();
    if let Err(error) = m.chat.dispatch(command) {
        if m.chat.session() == Some(session) {
            m.chat.status = error;
        }
    }
}
pub fn runtime_error(world: &mut World, session: &str, error: String) {
    conversations::with_session(world, session, |world| {
        runtime_error_current(world, session, error)
    });
}
fn runtime_error_current(world: &mut World, session: &str, error: String) {
    let m = world.resource_mut::<Model>().unwrap();
    if m.chat.session() == Some(session) {
        m.chat.status = error;
    }
}

pub(crate) fn active_chat_session(world: &World) -> Option<String> {
    world
        .resource::<Model>()
        .and_then(|m| m.chat.session().map(str::to_owned))
}

pub(crate) fn bot_identity_left(world: &World, width: f32) -> Option<f32> {
    let ui = world.resource::<Ui>()?;
    let bounds = world.get::<Rect>(ui.timeline)?;
    (bounds.w >= width).then_some(bounds.x + (bounds.w - width) / 2.)
}

pub(crate) fn bot_composer_identity(world: &mut World, name: Option<&str>) {
    let Some(editor) = world.resource::<Ui>().map(|ui| ui.composer.editor) else {
        return;
    };
    world.insert(
        editor,
        Placeholder::new(
            name.map(|n| format!("Message {n}…"))
                .unwrap_or_else(|| "Message Medousa…".into()),
        ),
    );
}
