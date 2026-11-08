//! App-owned chat presentation. Call `sync` every Update (also with unchanged
//! messages), after input scroll handling. MarkdownPlugin remains app-owned.
use rndr_core::*;

const PAD: f32 = 16.0; // rndr markdown's internal measurement inset
const MAX_WIDTH: f32 = 760.0;

struct Message {
    row: Entity,
    lane: Entity,
    bubble: Entity,
    view: Entity,
    role: String,
    // Only mounted user prompts need a literal-text cache. Assistant source is
    // owned by MarkdownView, not duplicated in the conversation projection.
    user_text: String,
    mounted: bool,
    height: f32,
    width: f32,
}
struct Conversation {
    messages: Vec<Message>,
    last_bottom: f32,
}
impl Component for Conversation {}
struct HistoryTrack(Entity, Entity);
impl Component for HistoryTrack {}

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
    // Keep the track outside the scrolling content so it never becomes a
    // history row or moves with the virtualized placeholders.
    let frame = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    set_parent(world, frame, parent);
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 12.0,
            gap: 18.0,
            ..Layout::column()
        },
        Scroll::default(),
        Conversation {
            messages: Vec::new(),
            last_bottom: 0.0,
        },
    ));
    set_parent(world, root, frame);
    // Reserve the hit lane even when no scrolling is needed; hiding the
    // control must not change wrapping or oscillate at the overflow boundary.
    let rail = world.spawn((Layout {
        width: Size::Px(24.0),
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(world, rail, frame);
    let track = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Scrollbar {
            target: root,
            axis: Axis::Column,
        },
        Focusable,
        Semantics {
            name: Some("Conversation history scroll position".into()),
            ..Default::default()
        },
    ));
    set_parent(world, track, rail);
    world.insert(root, HistoryTrack(track, frame));
    root
}

/// Roles are `user` and `assistant`; unknown roles use assistant presentation.
/// Streaming suffixes append to the existing MarkdownView, retaining its parsed
/// prefix and selection. Replacements retain row identity but reset selection.
#[cfg(test)]
pub fn sync(world: &mut World, root: Entity, messages: &[(String, String)]) {
    sync_iter(
        world,
        root,
        messages.iter().map(|(r, t)| (r.as_str(), t.as_str())),
    );
}

/// Keep lightweight height placeholders for history, but mount expensive Markdown
/// subtrees only around the viewport. Unknown heights are estimates until visited.
/// Borrow canonical text: idle updates never clone the full conversation.
pub fn sync_iter<'a>(
    world: &mut World,
    root: Entity,
    messages: impl IntoIterator<Item = (&'a str, &'a str)>,
) {
    let messages: Vec<_> = messages.into_iter().collect();
    let Some(mut state) = world.remove::<Conversation>(root) else {
        return;
    };
    let rect = world.get::<Rect>(root).copied().unwrap_or_default();
    let available = if rect.w > 24.0 {
        rect.w - 24.0
    } else {
        MAX_WIDTH
    };
    let ui_theme = world
        .resource::<rndr_ui::UiTheme>()
        .cloned()
        .unwrap_or_default();
    let Size::Px(width) = rndr_ui::reading_layout_at_measure(available, 75., &ui_theme).width
    else {
        unreachable!("rndr reading layout returns a point width");
    };
    let scroll = world.get::<Scroll>(root).copied().unwrap_or_default();
    let bottom = (scroll.content[1] - rect.h).max(0.0);
    if let Some(&HistoryTrack(track, frame)) = world.get::<HistoryTrack>(root) {
        if messages.is_empty() {
            world.insert(frame, Hidden);
        } else {
            world.remove::<Hidden>(frame);
        }
        if bottom > 1.5 && !messages.is_empty() {
            world.remove::<Hidden>(track);
        } else {
            // GenericInteraction reconciles focus when this control disappears.
            world.insert(track, Hidden);
        }
    }
    let follow = follows(scroll.offset[1], state.last_bottom, bottom);
    state.last_bottom = bottom;
    // Cache the last completed layout before evicting any subtree. Preserve the
    // viewport's anchor when an estimated row above it acquires a real height.
    let mut y = 12.0;
    let mut correction = 0.0;
    for message in &mut state.messages {
        if message.mounted
            && let Some(row) = world.get::<Rect>(message.row).filter(|r| r.h > 0.0)
        {
            if y + message.height < -scroll.offset[1] {
                correction += row.h - message.height;
            }
            message.height = row.h;
        }
        y += message.height + 18.0;
    }
    let offset = (scroll.offset[1] - correction).min(0.0);
    // A role change invalidates the suffix, not the unaffected prefix.
    let keep = state
        .messages
        .iter()
        .zip(&messages)
        .position(|(old, (role, _))| old.role != *role)
        .unwrap_or(state.messages.len().min(messages.len()));
    for old in state.messages.drain(keep..) {
        despawn(world, old.row);
    }
    for &(role, _) in &messages[state.messages.len()..] {
        let row = world.spawn((Layout {
            width: Size::Fill,
            height: Size::Px(160.0),
            ..Layout::row()
        },));
        set_parent(world, row, root);
        state.messages.push(Message {
            row,
            lane: row,
            bubble: row,
            view: row,
            role: role.into(),
            user_text: String::new(),
            mounted: false,
            height: 160.0,
            width: 0.0,
        });
    }
    let total: f32 = 24.0 + state.messages.iter().map(|m| m.height + 18.0).sum::<f32>();
    let top = if follow {
        (total - rect.h).max(0.0)
    } else {
        -offset
    };
    let overscan = rect.h.max(400.0);
    let mut y = 12.0;
    for (index, &(role, text)) in messages.iter().enumerate() {
        let message = &mut state.messages[index];
        let visible = y + message.height >= top - overscan && y <= top + rect.h + overscan;
        y += message.height + 18.0;
        if !visible {
            if message.mounted {
                for child in children_of(world, message.row) {
                    despawn(world, child);
                }
                message.mounted = false;
                message.user_text.clear();
                message.user_text.shrink_to_fit();
                world.get_mut::<Layout>(message.row).unwrap().height = Size::Px(message.height);
            }
            continue;
        }
        if !message.mounted {
            let height = message.height;
            *message = make_message(world, message.row, role, index);
            message.height = height;
        }
        world.get_mut::<Layout>(message.lane).unwrap().width = Size::Px(width);
        let user = role == "user";
        let changed = if user {
            message.user_text != text
        } else {
            world.get::<MarkdownView>(message.view).unwrap().source() != text
        };
        if !changed && message.width == width {
            continue;
        }
        message.width = width;
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
        if changed {
            let literal;
            let source = if user {
                message.user_text = text.into();
                literal = literal_markdown(text);
                literal.as_str()
            } else {
                text
            };
            let view = world.get_mut::<MarkdownView>(message.view).unwrap();
            if let Some(suffix) = source.strip_prefix(view.source()) {
                view.push_str(suffix);
            } else {
                view.set_markdown(source);
            }
        }
    }
    if let Some(scroll) = world.get_mut::<Scroll>(root) {
        scroll.offset[1] = if follow { f32::MIN } else { offset };
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
    if !message.mounted || message.role != "assistant" {
        return false;
    }
    let text = world
        .get::<MarkdownView>(message.view)
        .unwrap()
        .source()
        .to_owned();
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

fn make_message(world: &mut World, row: Entity, role: &str, index: usize) -> Message {
    world.get_mut::<Layout>(row).unwrap().height = Size::Hug;
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
        world.insert(
            bubble,
            rndr_ui::UiStyle {
                background: Some(rndr_ui::Token::Raised.into()),
                ..Default::default()
            },
        );
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
        world.insert(
            copy,
            rndr_ui::UiStyle {
                foreground: Some(rndr_ui::Token::Secondary.into()),
                ..Default::default()
            },
        );
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
        user_text: String::new(),
        mounted: true,
        height: 160.0,
        width: 0.0,
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
        assert!(!user.mounted, "offscreen user row should be a placeholder");
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
        let user = &app.world().get::<Conversation>(root).unwrap().messages[0];
        assert!(user.mounted);
        assert!(app.world().get::<Rect>(user.bubble).unwrap().w < 200.0);
        assert!(app.world().get::<Rect>(user.bubble).unwrap().x > 250.0);
    }
    #[test]
    fn thousand_message_history_mounts_only_viewport_and_recycles_on_scroll() {
        let mut app = App::new();
        app.add_plugin(MarkdownPlugin);
        let parent = app.world_mut().spawn((Layout {
            width: Size::Px(520.0),
            height: Size::Px(300.0),
            ..Layout::column()
        },));
        let root = setup(app.world_mut(), parent);
        let mut messages: Vec<_> = (0..1000)
            .map(|i| {
                (
                    "assistant".into(),
                    format!("Message {i}\n\n{}", "A paragraph.\n\n".repeat(8)),
                )
            })
            .collect();
        for _ in 0..8 {
            sync(app.world_mut(), root, &messages);
            app.update();
        }
        assert!(app.world().get::<Conversation>(root).unwrap().messages[999].mounted);
        assert!(app.world().entities_with::<MarkdownView>().len() < 20);
        assert!(!copy_message(app.world_mut(), root, "copy-message:0"));
        assert!(copy_message(app.world_mut(), root, "copy-message:999"));
        assert_eq!(
            app.world().resource::<Clipboard>().unwrap().text,
            messages[999].1
        );
        // Visit distant windows repeatedly. Eviction must free Markdown/layout
        // descendants, not hide them or grow a cache of previously visited rows.
        for offset in [0.0, -50000.0, -100000.0, 0.0] {
            app.world_mut().get_mut::<Scroll>(root).unwrap().offset[1] = offset;
            for _ in 0..8 {
                sync(app.world_mut(), root, &messages);
                app.update();
            }
            assert!(app.world().entities_with::<MarkdownView>().len() < 20);
            assert!(app.world().entities_with::<Layout>().len() < 2000);
        }
        assert!(app.world().get::<Conversation>(root).unwrap().messages[0].mounted);
        assert!(!app.world().get::<Conversation>(root).unwrap().messages[999].mounted);
        // A tail update must neither mount the tail nor pull a reader down.
        messages[999].1.push_str(" streaming tail");
        sync(app.world_mut(), root, &messages);
        app.update();
        assert_eq!(app.world().get::<Scroll>(root).unwrap().offset[1], 0.0);
        app.world_mut().get_mut::<Layout>(parent).unwrap().width = Size::Px(350.0);
        for _ in 0..5 {
            sync(app.world_mut(), root, &messages);
            app.update();
        }
        assert!(copy_message(app.world_mut(), root, "copy-message:0"));
        assert_eq!(
            app.world().resource::<Clipboard>().unwrap().text,
            messages[0].1
        );
        sync(app.world_mut(), root, &[]);
        app.update();
        assert!(app.world().entities_with::<MarkdownView>().is_empty());
        assert!(children_of(app.world(), root).is_empty());
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
