use super::*;
use crate::native_style as visual;
use rndr_ui::{Typography, text as ui_text};
fn panel(w: &mut World, p: Entity, l: Layout) -> Entity {
    let e = w.spawn((l,));
    set_parent(w, e, p);
    e
}
fn col(w: &mut World, p: Entity) -> Entity {
    panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::column()
        },
    )
}
fn row(w: &mut World, p: Entity) -> Entity {
    let e = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    w.insert(e, Wrap { line_gap: 6. });
    e
}
fn label(w: &mut World, p: Entity, t: &str, k: Typography) -> Entity {
    let e = ui_text(w, p, t, k).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
    e
}
fn button(w: &mut World, p: Entity, s: &mut State, t: &str, a: Action, enabled: bool) -> Entity {
    let id = next();
    let h = visual::document_action(w, p, id, t, enabled);
    s.actions.insert(id, a);
    h.root
}
fn field(w: &mut World, p: Entity, name: &str, value: &str, multi: bool) -> Entity {
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(if multi { 88. } else { 36. }),
            padding: 10.,
            ..Layout::column()
        },
        Label::new(value),
        Focusable,
        Prompt {
            submit_on_enter: false,
            max_lines: if multi { 4 } else { 1 },
            ..Default::default()
        },
        Placeholder::new(name),
        Semantics {
            name: Some(name.into()),
            ..Default::default()
        },
    ));
    set_parent(w, e, p);
    visual::plane(w, e, visual::raised());
    w.insert(e, Radius(8.));
    e
}
fn clear(w: &mut World, p: Entity) {
    if w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| crate::surfaces::descendant(w, e, p))
    {
        w.resource_mut::<Focus>().unwrap().entity = None;
    }
    for e in children_of(w, p) {
        despawn(w, e);
    }
}
pub(super) fn mount(w: &mut World, parent: Entity) {
    let root = panel(
        w,
        parent,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 1.,
            ..Layout::row()
        },
    );
    visual::flat(w, root, visual::canvas());
    crate::peer_management::mount(w, root);
    let sidebar = panel(
        w,
        root,
        Layout {
            width: Size::Px(284.),
            height: Size::Fill,
            padding: 16.,
            gap: 12.,
            ..Layout::column()
        },
    );
    visual::plane(w, sidebar, visual::navigation());
    w.insert(sidebar, visual::SidebarPane);
    label(w, sidebar, "Peers", Typography::Title);
    label(
        w,
        sidebar,
        "People & trusted workshops",
        Typography::Secondary,
    );
    let search = field(w, sidebar, "Search trusted peers", "", false);
    let controls = row(w, sidebar);
    let list = panel(
        w,
        sidebar,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 8.,
            ..Layout::column()
        },
    );
    w.insert(list, Scroll::default());
    let detail = panel(
        w,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 20.,
            gap: 14.,
            ..Layout::column()
        },
    );
    let header = col(w, detail);
    let thread = panel(
        w,
        detail,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 12.,
            padding: 8.,
            ..Layout::column()
        },
    );
    w.insert(thread, Scroll::default());
    let compose = col(w, detail);
    let mut s = empty(root);
    s.sidebar = sidebar;
    s.detail = detail;
    s.search = search;
    s.list = list;
    s.header = header;
    s.thread = thread;
    s.compose = compose;
    button(w, controls, &mut s, "Search", Action::Search, true);
    button(w, controls, &mut s, "Clear", Action::Clear, true);
    button(w, controls, &mut s, "Refresh", Action::Refresh, true);
    button(w, controls, &mut s, "Connections…", Action::Manage, true);
    s.persistent = s.actions.keys().copied().collect();
    w.insert_resource(s);
}
pub(super) fn render(w: &mut World, s: &mut State) {
    // Keep search/editor entities and their selection/undo/IME across responses.
    // Only ephemeral actions are removed; persistent search/send remain registered.
    let persistent: Vec<_> = s
        .actions
        .iter()
        .filter(|(id, a)| s.persistent.contains(id) || matches!(a, Action::Send))
        .map(|(id, a)| (*id, a.clone()))
        .collect();
    s.actions = persistent.into_iter().collect();
    clear(w, s.list);
    clear(w, s.header);
    clear(w, s.thread);
    if s.managing {
        w.insert(s.sidebar, Hidden);
        w.insert(s.detail, Hidden);
        return;
    }
    let list_only = s.narrow && s.selected.is_none();
    if s.narrow && !list_only {
        w.insert(s.sidebar, Hidden);
    } else {
        w.remove::<Hidden>(s.sidebar);
    }
    if list_only {
        w.insert(s.detail, Hidden);
    } else {
        w.remove::<Hidden>(s.detail);
    }
    w.get_mut::<Layout>(s.sidebar).unwrap().width = if list_only {
        Size::Fill
    } else {
        Size::Px(284.)
    };
    w.get_mut::<Layout>(s.detail).unwrap().padding = if s.narrow { 12. } else { 24. };
    if s.read.is_some() {
        label(w, s.list, "Refreshing…", Typography::Secondary);
    }
    let peers = s.peers.clone();
    let mut count = 0;
    for p in &peers {
        if !format!("{} {}", p.name, p.device)
            .to_lowercase()
            .contains(&s.query)
        {
            continue;
        }
        count += 1;
        let tile = col(w, s.list);
        w.get_mut::<Layout>(tile).unwrap().padding = 8.;
        w.insert(tile, Radius(8.));
        visual::plane(
            w,
            tile,
            if s.selected.as_ref() == Some(&p.id) {
                visual::raised()
            } else {
                visual::navigation()
            },
        );
        let n = s
            .messages
            .iter()
            .filter(|m| involves(m, p) && unread(m))
            .count();
        let title = if n > 0 {
            format!("{} · {n} unread", p.name)
        } else {
            p.name.clone()
        };
        let b = button(w, tile, s, &title, Action::Select(p.id.clone()), true);
        w.get_mut::<Layout>(b).unwrap().width = Size::Fill;
        label(
            w,
            tile,
            if p.inbound {
                "Connected to you"
            } else if p.ready {
                "Paired workshop"
            } else {
                "Needs reconnect"
            },
            Typography::Secondary,
        );
        if let Some(m) = s.messages.iter().rev().find(|m| involves(m, p)) {
            let preview = text(m, "body").replace('\n', " ");
            let e = label(w, tile, &preview, Typography::Secondary);
            w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Ellipsis;
            w.get_mut::<Layout>(e).unwrap().height = Size::Px(20.);
        }
        if s.drafts.get(&p.id).is_some_and(|d| !d.is_empty()) {
            label(w, tile, "Draft", Typography::Secondary);
        }
    }
    if count == 0 {
        label(
            w,
            s.list,
            if s.peers.is_empty() {
                "No trusted peers yet. Open Connections to pair a workshop."
            } else {
                "No peers match your search."
            },
            Typography::Secondary,
        );
    }
    if s.narrow && s.selected.is_some() {
        button(w, s.header, s, "‹ All peers", Action::Back, true);
    }
    let selected = s.peer().cloned();
    if let Some(bundle) = s.attachment.clone() {
        label(
            w,
            s.header,
            "Imported attachment · read-only",
            Typography::Title,
        );
        button(
            w,
            s.header,
            s,
            "Back to conversation",
            Action::CloseAttachment,
            true,
        );
        label(
            w,
            s.thread,
            "Source workshop content only. HTML/scripts are never executed. Preview limited to 32,000 characters per item.",
            Typography::Secondary,
        );
        for note in bundle["vaultNotes"].as_array().into_iter().flatten() {
            label(w, s.thread, &text(note, "path"), Typography::Title);
            label(
                w,
                s.thread,
                &text(note, "content")
                    .chars()
                    .take(32_000)
                    .collect::<String>(),
                Typography::Secondary,
            );
        }
        for item in bundle["artifacts"].as_array().into_iter().flatten() {
            use base64::Engine;
            label(
                w,
                s.thread,
                &format!("{} · {}", text(item, "title"), text(item, "mime")),
                Typography::Title,
            );
            let encoded = text(item, "contentBase64");
            let preview = if encoded.len() > 2 * 1024 * 1024 {
                "Artifact is too large for inline preview.".into()
            } else {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .ok()
                    .and_then(|v| String::from_utf8(v).ok())
                    .unwrap_or_else(|| "Binary artifact. Inline text preview unavailable.".into())
            };
            label(
                w,
                s.thread,
                &preview.chars().take(32_000).collect::<String>(),
                Typography::Secondary,
            );
        }
    } else if let Some(p) = &selected {
        label(w, s.header, &p.name, Typography::Title);
        label(
            w,
            s.header,
            &format!(
                "{} · {}",
                if p.inbound {
                    "Connected to you"
                } else if p.ready {
                    "Paired workshop · availability checked on send"
                } else {
                    "Open Connections to pair again"
                },
                p.device
            ),
            Typography::Secondary,
        );
        let bar = row(w, s.header);
        button(
            w,
            bar,
            s,
            "Refresh",
            Action::Refresh,
            s.read.is_none() && s.mutation.is_none(),
        );
        let has_unread = s.messages.iter().any(|m| involves(m, p) && unread(m));
        button(
            w,
            bar,
            s,
            "Mark read",
            Action::MarkRead,
            has_unread && s.mutation.is_none(),
        );
        let messages = s
            .messages
            .iter()
            .filter(|m| involves(m, p))
            .cloned()
            .collect::<Vec<_>>();
        if messages.is_empty() {
            label(w, s.thread, "Start a conversation", Typography::Title);
            label(
                w,
                s.thread,
                "Messages stay between these workshops. Sending does not start an agent or grant access to your workspace.",
                Typography::Secondary,
            );
        }
        let mut date = String::new();
        for m in messages {
            let stamp = text(&m, "sentAt");
            let day = stamp.chars().take(10).collect::<String>();
            if day != date {
                label(w, s.thread, &day, Typography::Secondary);
                date = day;
            }
            let outbound = m["direction"].as_str() == Some("out");
            let lane = panel(
                w,
                s.thread,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,

                    ..Layout::row()
                },
            );
            if outbound {
                panel(
                    w,
                    lane,
                    Layout {
                        width: Size::Fill,
                        ..Layout::row()
                    },
                );
            }
            let bubble = panel(
                w,
                lane,
                Layout {
                    width: Size::Px(
                        ((w.resource::<Viewport>().map(|v| v.width).unwrap_or(1200.)
                            - 56.
                            - if s.narrow { 0. } else { 284. }
                            - 64.)
                            * 0.88)
                            .clamp(120., 720.),
                    ),
                    height: Size::Hug,
                    padding: 12.,
                    gap: 6.,
                    ..Layout::column()
                },
            );
            visual::plane(
                w,
                bubble,
                if outbound {
                    visual::raised()
                } else {
                    visual::navigation()
                },
            );
            w.insert(bubble, Radius(10.));
            let who = if outbound {
                "You".into()
            } else {
                p.name.clone()
            };
            let time = chrono::DateTime::parse_from_rfc3339(&stamp)
                .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or(stamp);
            label(
                w,
                bubble,
                &format!(
                    "{who} · {time}{}",
                    if unread(&m) { " · unread" } else { "" }
                ),
                Typography::Secondary,
            );
            label(w, bubble, &text(&m, "body"), Typography::Reading);
            if let Some(kind) = m["kind"].as_str() {
                label(
                    w,
                    bubble,
                    &format!("Message type: {kind}"),
                    Typography::Secondary,
                );
            }
            if !m["attachment"].is_null() {
                label(w, bubble, "Shared attachment", Typography::Secondary);
                if let Some(summary) = m["attachmentResult"]["summary"].as_str() {
                    label(w, bubble, summary, Typography::Secondary);
                }
                let result = &m["attachmentResult"];
                let mut count = 0;
                if result["imported"] == true {
                    for (key, artifact) in [("vaultPaths", false), ("artifactIds", true)] {
                        for item in result[key]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                        {
                            button(
                                w,
                                bubble,
                                s,
                                &format!(
                                    "Open {}: {item}",
                                    if artifact { "artifact" } else { "note" }
                                ),
                                Action::OpenAttachment(m.clone(), item.into(), artifact),
                                s.read.is_none() && s.mutation.is_none(),
                            );
                            count += 1;
                        }
                    }
                    if count == 0 {
                        label(
                            w,
                            bubble,
                            "This older message has no imported item IDs. Locate it in the source workshop; it will not be re-imported automatically.",
                            Typography::Secondary,
                        );
                    }
                }
            }
        }
        if s.scroll_peer.as_ref() != Some(&p.id) {
            w.get_mut::<Scroll>(s.thread).unwrap().offset[1] = -1e8;
            s.scroll_peer = Some(p.id.clone());
        }
    } else {
        label(
            w,
            s.header,
            "Your people, one conversation away",
            Typography::Title,
        );
        label(
            w,
            s.thread,
            "Choose a trusted peer to read and send messages.",
            Typography::Secondary,
        );
    }
    label(w, s.header, &s.status, Typography::Secondary);
    for warning in &s.warnings {
        label(w, s.header, warning, Typography::Secondary);
    }
    if s.selected
        .as_ref()
        .is_some_and(|id| s.uncertain.contains_key(id))
    {
        label(
            w,
            s.header,
            "Delivery may have succeeded. Refresh and check the thread before choosing to send again.",
            Typography::Secondary,
        );
        button(
            w,
            s.header,
            s,
            "I checked · allow another send",
            Action::AllowResend,
            s.read.is_none(),
        );
    }
    if s.composer_peer != s.selected {
        clear(w, s.compose);
        s.actions.retain(|_, a| !matches!(a, Action::Send));
        s.editor = None;
        s.send = None;
        s.composer_peer = s.selected.clone();
        if selected.is_some() {
            s.editor = Some(field(w, s.compose, "Message peer", s.draft(), true));
            let bar = row(w, s.compose);
            label(
                w,
                bar,
                "Drafts stay in this window · explicit Send",
                Typography::Secondary,
            );
            s.send = Some(button(w, bar, s, "Send", Action::Send, false));
        }
    }
    if selected.is_some() && s.attachment.is_none() {
        w.remove::<Hidden>(s.compose);
    } else {
        w.insert(s.compose, Hidden);
    }
}
