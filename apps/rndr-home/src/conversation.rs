//! App-owned chat presentation. Call `sync` every Update (also with unchanged
//! messages), after input scroll handling. MarkdownPlugin remains app-owned.
use rndr_core::*;

const PAD: f32 = 16.0; // rndr markdown's internal measurement inset
const MAX_WIDTH: f32 = 760.0;
const BUBBLE: Color = Color::rgb(0.17, 0.17, 0.19);

struct Message {
    row: Entity,
    lane: Entity,
    bubble: Entity,
    view: Entity,
    role: String,
    source: String,
}
struct Conversation {
    messages: Vec<Message>,
    last_bottom: f32,
}
impl Component for Conversation {}

/// A single scrollport: message views hug their content and never capture wheel
/// input. Selection/copy is provided by MarkdownPlugin within each message.
pub fn setup(world: &mut World, parent: Entity) -> Entity {
    // Markdown paints its own surface on every sync (including paragraph fills).
    // Transparency must therefore be in its theme, not a removed Color component.
    let mut theme = world
        .resource::<MarkdownTheme>()
        .copied()
        .unwrap_or_else(|| MarkdownTheme::from_theme(&theme_of(world)));
    theme.surface = Color::rgba(0.0, 0.0, 0.0, 0.0);
    world.insert_resource(theme);
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 12.0,
            gap: 18.0,
            ..Layout::column()
        },
        Scroll::default(),
        ScrollIndicator {
            color: Color::rgba(0.55, 0.55, 0.6, 0.4),
        },
        Conversation {
            messages: Vec::new(),
            last_bottom: 0.0,
        },
    ));
    set_parent(world, root, parent);
    root
}

/// Roles are `user` and `assistant`; unknown roles use assistant presentation.
/// Streaming suffixes append to the existing MarkdownView, retaining its parsed
/// prefix and selection. Replacements retain row identity but reset selection.
pub fn sync(world: &mut World, root: Entity, messages: &[(String, String)]) {
    let Some(mut state) = world.remove::<Conversation>(root) else {
        return;
    };
    let rect = world.get::<Rect>(root).copied().unwrap_or_default();
    let width = if rect.w > 24.0 {
        (rect.w - 24.0).min(MAX_WIDTH)
    } else {
        MAX_WIDTH
    };
    if let Some(scroll) = world.get_mut::<Scroll>(root) {
        let bottom = (scroll.content[1] - rect.h).max(0.0);
        // Compare against the preceding extent too: newly laid-out streaming
        // content may have grown since Update. A real upward user scroll still
        // unpins; returning to the bottom resumes following automatically.
        if follows(scroll.offset[1], state.last_bottom, bottom) {
            scroll.offset[1] = f32::MIN;
        }
        state.last_bottom = bottom;
    }
    // A role change invalidates the suffix, not the unaffected prefix.
    let keep = state
        .messages
        .iter()
        .zip(messages)
        .position(|(old, (role, _))| old.role != *role)
        .unwrap_or(state.messages.len().min(messages.len()));
    for old in state.messages.drain(keep..) {
        despawn(world, old.row);
    }
    for (index, (role, text)) in messages.iter().enumerate() {
        if index == state.messages.len() {
            state.messages.push(make_message(world, root, role, index));
        }
        let message = &mut state.messages[index];
        world.get_mut::<Layout>(message.lane).unwrap().width = Size::Px(width);
        let user = role == "user";
        let body_width = if user {
            let measured = text_engine(world)
                .measure(text, &style_of(world), &[], f32::INFINITY)
                .width;
            (measured + PAD * 2.0 + 2.0)
                .max(64.0)
                .min((width * 0.82).min(560.0))
        } else {
            width
        };
        world.get_mut::<Layout>(message.bubble).unwrap().width = Size::Px(body_width);
        let source = if user {
            literal_markdown(text)
        } else {
            text.clone()
        };
        if source != message.source {
            let view = world.get_mut::<MarkdownView>(message.view).unwrap();
            if let Some(suffix) = source.strip_prefix(&message.source) {
                view.push_str(suffix);
            } else {
                view.set_markdown(&source);
            }
            message.source = source;
        }
    }
    world.insert(root, state);
}

/// Copy only the currently mounted assistant body. Never dispatch commands or
/// infer actions from response text. The native runner owns OS clipboard I/O.
pub fn copy_message(world: &mut World, root: Entity, action: &str) -> bool {
    let Some(index) = action
        .strip_prefix("copy-message:")
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return false;
    };
    let Some(message) = world
        .get::<Conversation>(root)
        .and_then(|s| s.messages.get(index))
    else {
        return false;
    };
    if message.role != "assistant" {
        return false;
    }
    let text = message.source.clone();
    if let Some(clipboard) = world.resource_mut::<Clipboard>() {
        clipboard.text = text;
        clipboard.dirty = true;
    } else {
        world.insert_resource(Clipboard {
            text,
            dirty: true,
            ..Default::default()
        });
    }
    true
}

fn follows(offset: f32, previous_bottom: f32, bottom: f32) -> bool {
    offset <= -previous_bottom + 1.5 || offset <= -bottom + 1.5
}

fn make_message(world: &mut World, root: Entity, role: &str, index: usize) -> Message {
    let row = world.spawn((Layout {
        width: Size::Fill,
        ..Layout::row()
    },));
    set_parent(world, row, root);
    let left = world.spawn((Layout {
        width: Size::Fill,
        ..Layout::column()
    },));
    set_parent(world, left, row);
    let lane = world.spawn((Layout {
        width: Size::Px(MAX_WIDTH),
        ..Layout::row()
    },));
    set_parent(world, lane, row);
    let right = world.spawn((Layout {
        width: Size::Fill,
        ..Layout::column()
    },));
    set_parent(world, right, row);
    if role == "user" {
        let spacer = world.spawn((Layout {
            width: Size::Fill,
            ..Layout::column()
        },));
        set_parent(world, spacer, lane);
    }
    let bubble = world.spawn((Layout {
        width: Size::Fill,
        ..Layout::column()
    },));
    set_parent(world, bubble, lane);
    if role == "user" {
        world.insert(bubble, BUBBLE);
        world.insert(bubble, Radius(16.0));
    }
    let view = markdown_view(world, "");
    world.get_mut::<Layout>(view).unwrap().height = Size::Hug;
    world.remove::<Scroll>(view);
    world
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    set_parent(world, view, bubble);
    if role == "assistant" {
        let copy = rndr_app::button(world, bubble, &format!("copy-message:{index}"), "Copy");
        world.insert(copy, Color::rgba(0., 0., 0., 0.));
        world.get_mut::<Layout>(copy).unwrap().width = Size::Px(56.);
        world.get_mut::<Layout>(copy).unwrap().height = Size::Px(24.);
        world.get_mut::<Layout>(copy).unwrap().min_height = 0.;
        world.get_mut::<TextStyle>(copy).unwrap().size = 12.;
        world.get_mut::<Label>(copy).unwrap().color = Some(Color::rgb(0.53, 0.53, 0.55));
        world.insert(
            copy,
            Semantics {
                name: Some("Copy assistant message as Markdown".into()),
                ..Default::default()
            },
        );
    }
    Message {
        row,
        lane,
        bubble,
        view,
        role: role.into(),
        source: String::new(),
    }
}

// User prompts are prose, not markdown. Preserve explicit newlines as hard
// breaks, and neutralize list/heading/code/link/html/table/autolink syntax.
fn literal_markdown(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c == '\n' {
            out.push_str("  \n");
        } else {
            if c.is_ascii_punctuation() {
                out.push('\\');
            }
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (World, Entity) {
        let mut world = World::new();
        let parent = world.spawn((Layout::column(),));
        let root = setup(&mut world, parent);
        world.insert(
            root,
            Rect {
                w: 520.0,
                h: 300.0,
                ..Rect::default()
            },
        );
        (world, root)
    }
    #[test]
    fn streaming_retains_rows_and_no_inner_scrollports() {
        let (mut world, root) = fixture();
        let mut messages = vec![
            ("user".into(), "hello".into()),
            ("assistant".into(), "answer".into()),
        ];
        sync(&mut world, root, &messages);
        let ids: Vec<_> = world
            .get::<Conversation>(root)
            .unwrap()
            .messages
            .iter()
            .map(|m| (m.row, m.view))
            .collect();
        messages[1].1.push_str(" streaming");
        sync(&mut world, root, &messages);
        for (message, (row, view)) in world
            .get::<Conversation>(root)
            .unwrap()
            .messages
            .iter()
            .zip(ids)
        {
            assert_eq!(message.row, row);
            assert_eq!(message.view, view);
            assert!(!world.has::<Scroll>(view));
            assert_eq!(world.get::<Layout>(view).unwrap().height, Size::Hug);
        }
        assert_eq!(world.entities_with::<Scroll>(), vec![root]);
    }
    #[test]
    fn truncation_cleans_subtrees_and_narrow_bubbles_fit() {
        let (mut world, root) = fixture();
        let messages = vec![
            ("user".into(), "a very long prompt ".repeat(100)),
            ("assistant".into(), "answer".into()),
        ];
        sync(&mut world, root, &messages);
        let state = world.get::<Conversation>(root).unwrap();
        let removed = state.messages[1].view;
        let Size::Px(width) = world.get::<Layout>(state.messages[0].bubble).unwrap().width else {
            panic!()
        };
        assert!(width <= (520.0 - 24.0) * 0.82);
        sync(&mut world, root, &messages[..1]);
        assert!(!world.is_alive(removed));
        sync(&mut world, root, &[]);
        assert!(children_of(&world, root).is_empty());
    }
    #[test]
    fn follow_respects_user_scroll_and_growth() {
        assert!(follows(-100.0, 100.0, 150.0));
        assert!(!follows(-70.0, 100.0, 150.0));
        assert!(follows(-150.0, 100.0, 150.0));
        assert!(follows(0.0, 0.0, 50.0));
    }
    #[test]
    fn layout_hugs_messages_and_outer_port_scrolls() {
        let mut app = App::new();
        app.add_plugin(MarkdownPlugin);
        let parent = app.world_mut().spawn((Layout {
            width: Size::Px(520.0),
            height: Size::Px(300.0),
            ..Layout::column()
        },));
        let root = setup(app.world_mut(), parent);
        let messages = vec![
            ("user".into(), "hello".into()),
            ("assistant".into(), "A paragraph of text.\n\n".repeat(30)),
        ];
        for _ in 0..5 {
            sync(app.world_mut(), root, &messages);
            app.update();
        }
        let world = app.world();
        let state = world.get::<Conversation>(root).unwrap();
        let user = &state.messages[0];
        let assistant = &state.messages[1];
        assert!(world.get::<Rect>(user.bubble).unwrap().w < 200.0);
        assert!(world.get::<Rect>(user.bubble).unwrap().x > 250.0);
        assert!(world.get::<Rect>(assistant.view).unwrap().h > 300.0);
        let scroll = world.get::<Scroll>(root).unwrap();
        let bottom = (scroll.content[1] - world.get::<Rect>(root).unwrap().h).max(0.0);
        assert!(bottom > 0.0);
        assert!((scroll.offset[1] + bottom).abs() < 2.0);
        app.world_mut().get_mut::<Scroll>(root).unwrap().offset[1] = -30.0;
        for _ in 0..3 {
            sync(app.world_mut(), root, &messages);
            app.update();
        }
        assert!((app.world().get::<Scroll>(root).unwrap().offset[1] + 30.0).abs() < 2.0);
    }
    #[test]
    fn prompt_markup_is_literal() {
        let source = literal_markdown("# title\n**hello** <tag> [x](url) | code `x`");
        let rendered: String = parse_markdown(&source)
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(rendered, "# title\n**hello** <tag> [x](url) | code `x`");
    }
}
