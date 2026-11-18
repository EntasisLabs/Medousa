use super::*;
use crate::native_style as visual;
use rndr_ui::{
    Typography, UiId,
    controls::{self, ButtonProps, NavigationMeta},
    text,
};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone)]
struct FileDiff(String, Revision);
impl Component for FileDiff {}
static NEXT: AtomicU64 = AtomicU64::new(0x5256_0000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn panel(w: &mut World, p: Entity, layout: Layout) -> Entity {
    let e = w.spawn((layout,));
    set_parent(w, e, p);
    e
}
fn column(w: &mut World, p: Entity) -> Entity {
    panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 12.,
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
fn caption(w: &mut World, p: Entity, s: &str) {
    let e = text(w, p, s, Typography::Secondary).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
}
fn heading(w: &mut World, p: Entity, s: &str) {
    let e = text(w, p, s, Typography::Emphasis).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
}
fn clear(w: &mut World, p: Entity) {
    if w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|f| crate::surfaces::descendant(w, f, p))
    {
        w.resource_mut::<Focus>().unwrap().entity = None;
    }
    for e in children_of(w, p) {
        despawn(w, e);
    }
}
fn button(w: &mut World, p: Entity, s: &mut State, label: &str, a: Action, enabled: bool) {
    let id = next();
    if matches!(label, "Approve" | "Confirm" | "Apply approved decision") {
        let mut props = ButtonProps::new(label, HostIntent::Command(id));
        props.enabled = enabled;
        props.variant = controls::ButtonVariant::Primary;
        let h = controls::Button::mount(w, p, UiId(id), props).unwrap();
        w.get_mut::<Layout>(h.root).unwrap().height = Size::Px(32.);
        w.insert(
            h.root,
            visual::FitLabel {
                label: h.label,
                extra: 8.,
                min: 80.,
                max: 240.,
            },
        );
    } else {
        visual::document_action(w, p, id, label, enabled);
    }
    s.actions.insert(id, a);
}
fn nav(w: &mut World, p: Entity, s: &mut State, label: &str, a: Action, selected: bool) {
    let id = next();
    let mut props = ButtonProps::new(label, HostIntent::Command(id));
    props.enabled = s.mutation_pending.is_none();
    let h = controls::NavigationRow::mount(
        w,
        p,
        UiId(id),
        props,
        NavigationMeta {
            selected,
            ..Default::default()
        },
    )
    .unwrap();
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(38.),
        ..Layout::column()
    };
    w.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
    s.actions.insert(id, a);
}
fn phase_label(phase: &str) -> &str {
    match phase {
        "review" | "needs_attention" => "Ready for review",
        "complete" => "Reviewed",
        "work" => "In progress",
        "prepare" => "Preparing",
        _ => phase,
    }
}
fn reading(w: &mut World, p: Entity, value: &str) {
    if value.is_empty() {
        return;
    }
    let e = text(w, p, value, Typography::Secondary).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
}
fn tab(w: &mut World, p: Entity, s: &mut State, label: &str, action: Action, selected: bool) {
    let holder = panel(
        w,
        p,
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            gap: 4.,
            ..Layout::column()
        },
    );
    let id = next();
    let h = visual::document_action(w, holder, id, label, s.mutation_pending.is_none());
    if selected {
        w.insert(
            h.label,
            rndr_ui::UiStyle {
                typography: Some(Typography::Emphasis),
                ..Default::default()
            },
        );
        let rule = visual::rule(w, holder);
        visual::plane(w, rule, visual::text());
    }
    s.actions.insert(id, action);
}
fn prompt(w: &mut World, p: Entity, name: &str, value: &str, height: f32) -> Entity {
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(height),
            padding: 6.,
            ..Layout::column()
        },
        Label::new(value),
        Focusable,
        Prompt {
            submit_on_enter: false,
            ..Prompt::new()
        },
        Placeholder::new(name),
        Semantics {
            name: Some(name.into()),
            ..Default::default()
        },
    ));
    set_parent(w, e, p);
    visual::plane(w, e, visual::rail());
    e
}
pub(super) fn mount(w: &mut World, parent: Entity) {
    let root = panel(
        w,
        parent,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
    );
    visual::flat(w, root, visual::canvas());
    let sidebar = panel(
        w,
        root,
        Layout {
            width: Size::Px(256.),
            height: Size::Fill,
            padding: 12.,
            gap: 8.,
            ..Layout::column()
        },
    );
    visual::navigation_frame(w, sidebar);
    heading(w, sidebar, "Reviews");
    let search = prompt(w, sidebar, "Search reviews", "", 38.);
    let queue_tools = column(w, sidebar);
    let queue_panel = panel(
        w,
        sidebar,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 5.,
            ..Layout::column()
        },
    );
    w.insert(queue_panel, Scroll::default());
    let main = panel(
        w,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 24.,
            gap: 16.,
            ..Layout::column()
        },
    );
    let detail = panel(
        w,
        main,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 18.,
            ..Layout::column()
        },
    );
    w.insert(detail, Scroll::default());
    let form = column(w, main);
    w.insert_resource(State {
        queue_panel: Some(queue_panel),
        queue_tools: Some(queue_tools),
        detail: Some(detail),
        form: Some(form),
        search: Some(search),
        redraw: true,
        ..Default::default()
    });
}
pub(super) fn render(w: &mut World, s: &mut State) {
    let (Some(q), Some(p), Some(form)) = (s.queue_panel, s.detail, s.form) else {
        return;
    };
    for (root, file) in w.query::<FileDiff>() {
        if revision(&s.review).as_ref() != Some(&file.1) { continue; }
        let handle = rndr_ui::collections::VirtualViewHandle { root };
        if let (Some(anchor), Some(scroll)) = (handle.anchor(w), w.get::<Scroll>(root)) {
            s.diff_positions.insert(file.0.clone(), (anchor, scroll.offset[0]));
        }
    }
    let tools = s.queue_tools.unwrap_or(q);
    if tools != q {
        clear(w, tools);
    }
    clear(w, q);
    clear(w, p);
    clear(w, form);
    s.actions.clear();
    s.body = None;
    s.line = None;
    let busy = s.mutation_pending.is_some();
    button(
        w,
        tools,
        s,
        if s.queue_pending.is_some() {
            "Refreshing…"
        } else {
            "Refresh"
        },
        Action::Refresh,
        !busy && s.queue_pending.is_none(),
    );
    let filters = row(w, tools);
    for (value, label) in [(0, "All"), (1, "Active"), (2, "Reviewed")] {
        tab(
            w,
            filters,
            s,
            label,
            Action::Filter(value),
            s.filter == value,
        );
    }
    let items = s.items.clone();
    let query = s.query.to_lowercase();
    for (name, phases) in [
        ("Ready for review", vec!["review", "needs_attention"]),
        ("In progress", vec!["prepare", "work"]),
        ("Reviewed", vec!["complete"]),
    ] {
        if (s.filter == 1 && name == "Reviewed") || (s.filter == 2 && name != "Reviewed") {
            continue;
        }
        let rows: Vec<_> = items
            .iter()
            .filter(|i| {
                phases.contains(&i["human_phase"].as_str().unwrap_or("prepare"))
                    && format!("{} {}", string(i, "title"), string(i, "id"))
                        .to_lowercase()
                        .contains(&query)
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        caption(w, q, &format!("{name} · {}", rows.len()));
        for item in rows {
            let id = string(item, "id");
            let entry = column(w, q);
            w.get_mut::<Layout>(entry).unwrap().gap = 2.;
            nav(
                w,
                entry,
                s,
                &string(item, "title"),
                Action::Open(id.clone()),
                s.work.as_ref() == Some(&id),
            );
            caption(
                w,
                entry,
                &format!("Forge · {}", phase_label(&string(item, "human_phase"))),
            );
        }
    }
    if items.is_empty() {
        caption(w, q, "No Forge undertakings yet.");
    }
    if !s.message.is_empty() {
        caption(w, p, &s.message);
    }
    if s.read_pending.is_some() {
        caption(w, p, "Loading sealed review…");
    }
    if s.review.is_null() {
        heading(w, p, "Review the work, not a moving checkout");
        caption(
            w,
            p,
            "Choose an undertaking to read its outcome, checks and sealed changes. Approval and apply are separate actions.",
        );
        return;
    }
    let review = s.review.clone();
    let usable = ready(s);
    caption(
        w,
        p,
        &format!("Forge  /  {}", phase_label(&string(&review, "human_phase"))),
    );
    let tabs = row(w, p);
    tab(w, tabs, s, "Summary", Action::Summary, !s.changes);
    let files = review["changed_files"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    tab(
        w,
        tabs,
        s,
        &format!("Changes  {}", files.len()),
        Action::Changes,
        s.changes,
    );
    visual::rule(w, p);
    if s.changes {
        heading(w, p, &string(&review, "title"));
        let tools = row(w, p);
        caption(w, p, &format!("{} changed file{}", files.len(), if files.len() == 1 { "" } else { "s" }));
        button(
            w,
            tools,
            s,
            "Expand all",
            Action::ExpandAll,
            !busy && s.read_pending.is_none(),
        );
        button(w, tools, s, "Collapse all", Action::CollapseAll, !busy);
        // One full-width stream. File headers remain ordinary navigation controls
        // (keyboard/semantics included); each bounded diff owns its two-axis port.
        for file in &files {
            let path = string(file, "path");
            let open = s.expanded.contains(&path);
            let section = column(w, p);
            visual::rule(w, section);
            let header = row(w, section);
            nav(
                w,
                header,
                s,
                &format!(
                    "{}  {}    +{}  −{}",
                    if open { "−" } else { "+" },
                    path,
                    file["lines_added"].as_u64().unwrap_or(0),
                    file["lines_removed"].as_u64().unwrap_or(0)
                ),
                Action::ToggleFile(path.clone()),
                false,
            );
            if !open {
                continue;
            }
            let selected = s.file.as_ref().is_some_and(|f| f.0 == path);
            let value = s.file_cache.get(&path).cloned().or_else(|| {
                s.file
                    .as_ref()
                    .filter(|f| f.0 == path && !f.1.is_null())
                    .map(|f| f.1.clone())
            });
            if s.file_errors.contains(&path) {
                caption(
                    w,
                    section,
                    "Sealed diff unavailable. Retry to read this exact revision.",
                );
                button(
                    w,
                    section,
                    s,
                    "Retry sealed diff",
                    Action::File(path.clone()),
                    !busy,
                );
            } else if let Some(value) = value {
                if value["binary"] == true {
                    caption(w, section, "Binary change · text diff unavailable.");
                } else {
                    if value["truncated"] == true {
                        caption(
                            w,
                            section,
                            "Truncated diff · only part of this file is shown.",
                        );
                    }
                    if value["hunks"].as_array().is_none_or(|h| h.is_empty()) {
                        caption(w, section, "No text changes in the sealed diff.");
                    } else {
                        let count: usize = value["hunks"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|h| 1 + h["lines"].as_array().map_or(0, Vec::len))
                            .sum();
                        let maximum = (w.resource::<Viewport>().map_or(850., |v| v.height) - 480.)
                            .clamp(180., 360.);
                        let height = (count as f32 * crate::diff_presentation::row_height(w) + 12.)
                            .min(maximum);
                        let holder = panel(
                            w,
                            section,
                            Layout {
                                width: Size::Fill,
                                height: Size::Px(height),
                                ..Layout::column()
                            },
                        );
                        let handle = crate::diff_presentation::mount(
                            w,
                            holder,
                            UiId(next()),
                            format!("Sealed changes to {path}"),
                            &value,
                        )
                        .unwrap();
                        w.insert(
                            handle.root,
                            FileDiff(path.clone(), revision(&review).unwrap()),
                        );
                        if let Some((anchor, x)) = s.diff_positions.get(&path) {
                            let _ = rndr_ui::collections::VirtualViewHandle { root: handle.root }
                                .jump(w, anchor.id, anchor.remainder_mp);
                            w.get_mut::<Scroll>(handle.root).unwrap().offset[0] = *x;
                        }
                    }
                }
                button(
                    w,
                    section,
                    s,
                    if selected {
                        "Commenting on this file"
                    } else {
                        "Comment on this file"
                    },
                    Action::File(path.clone()),
                    !busy && !selected,
                );
            } else {
                caption(
                    w,
                    section,
                    if s.loading_path.as_deref() == Some(&path) {
                        "Loading exact sealed diff…"
                    } else {
                        "Waiting to load exact sealed diff…"
                    },
                );
            }
        }
    } else {
        let synthesis = &review["synthesis"];
        let wide = w.resource::<Viewport>().map_or(1200., |v| v.width) >= 1100.;
        let content = panel(
            w,
            p,
            Layout {
                width: Size::Fill,
                height: Size::Hug,
                gap: 32.,
                ..if wide {
                    Layout::row()
                } else {
                    Layout::column()
                }
            },
        );
        let article = column(w, content);
        let title = text(w, article, &string(&review, "title"), Typography::Heading).unwrap();
        w.get_mut::<Layout>(title).unwrap().width = Size::Fill;
        w.get_mut::<Label>(title).unwrap().overflow = TextOverflow::Wrap;
        let candidate = review["candidates"]
            .as_array()
            .and_then(|a| a.iter().find(|c| c["attempt_id"] == review["attempt_id"]));
        let author = candidate.map(|c| string(c, "executor")).unwrap_or_default();
        caption(
            w,
            article,
            &format!(
                "{}{}Revision {}",
                author,
                if author.is_empty() { "" } else { " · " },
                review["attempt_seq"]
            ),
        );
        if let Some(candidate) = candidate {
            let branch = string(candidate, "branch");
            if !branch.is_empty() {
                caption(w, article, &branch);
            }
        }
        reading(w, article, &string(synthesis, "outcome"));
        heading(w, article, "Changed areas");
        let mut areas = std::collections::BTreeMap::<String, usize>::new();
        for file in &files {
            let path = string(file, "path");
            let area = path
                .split_once('/')
                .map_or("Root files".into(), |(a, _)| format!("{a}/"));
            *areas.entry(area).or_default() += 1;
        }
        for (area, count) in areas {
            visual::rule(w, article);
            let area_row = column(w, article);
            w.get_mut::<Layout>(area_row).unwrap().gap = 2.;
            button(w, area_row, s, &area, Action::Changes, !busy);
            let paths = files
                .iter()
                .filter_map(|f| f["path"].as_str())
                .filter(|path| {
                    if area == "Root files" {
                        !path.contains('/')
                    } else {
                        path.starts_with(&area)
                    }
                })
                .take(3)
                .collect::<Vec<_>>()
                .join(", ");
            let preview = text(w, area_row, &paths, Typography::Caption).unwrap();
            w.get_mut::<Layout>(preview).unwrap().width = Size::Fill;
            w.get_mut::<Label>(preview).unwrap().overflow = TextOverflow::Ellipsis;
            caption(
                w,
                area_row,
                &format!(
                    "{count} changed {}",
                    if count == 1 { "file" } else { "files" }
                ),
            );
        }
        visual::rule(w, article);
        heading(w, article, "Review status");
        reading(w, article, &string(synthesis, "status_summary"));
        caption(w, article, &string(synthesis, "recommended_next_action"));
        if let Some(rationale) = review["decision"]["rationale"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            heading(w, article, "Review decision");
            reading(w, article, rationale);
        }
        let meta = column(w, content);
        if wide {
            w.get_mut::<Layout>(meta).unwrap().width = Size::Px(224.);
        }
        caption(w, meta, "Changes");
        let added: u64 = files.iter().filter_map(|f| f["lines_added"].as_u64()).sum();
        let removed: u64 = files
            .iter()
            .filter_map(|f| f["lines_removed"].as_u64())
            .sum();
        heading(
            w,
            meta,
            &format!("{} files   +{added}  −{removed}", files.len()),
        );
        visual::rule(w, meta);
        caption(w, meta, "Checks");
        if synthesis["verification"].is_object() {
            let v = &synthesis["verification"];
            heading(
                w,
                meta,
                if v["success"] == true {
                    "Passed"
                } else {
                    "Not passed"
                },
            );
            caption(w, meta, &string(v, "label"));
            caption(
                w,
                meta,
                &v["command"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default(),
            );
        } else {
            caption(w, meta, "No checks recorded");
        }
        visual::rule(w, meta);
        caption(w, meta, "Risk");
        heading(w, meta, &string(synthesis, "risk"));
        caption(w, meta, &string(synthesis, "risk_summary"));
        visual::rule(w, meta);
        caption(w, meta, "Candidate");
        let candidates = review["candidates"].as_array().cloned().unwrap_or_default();
        if candidates.len() > 1 {
            for c in candidates {
                button(
                    w,
                    meta,
                    s,
                    &format!(
                        "Revision {}{}",
                        c["attempt_seq"],
                        if c["attempt_id"] == review["attempt_id"] {
                            " · reading"
                        } else {
                            ""
                        }
                    ),
                    Action::Candidate(string(&c, "attempt_id")),
                    !busy && s.read_pending.is_none(),
                );
            }
        } else {
            caption(w, meta, &format!("Revision {}", review["attempt_seq"]));
        }
        for issue in synthesis["unresolved_issues"]
            .as_array()
            .into_iter()
            .flatten()
        {
            caption(w, p, issue.as_str().unwrap_or("Review issue"));
        }
        for issue in synthesis["issues"].as_array().into_iter().flatten() {
            caption(
                w,
                p,
                &format!(
                    "{}{}",
                    if issue["blocks_approval"] == true {
                        "Blocks approval · "
                    } else {
                        ""
                    },
                    string(issue, "message")
                ),
            );
        }
        if review["base_advanced"] == true {
            caption(
                w,
                p,
                "Starting branch advanced. Refresh/reseal before approval.",
            );
        }
        if review["truncated"] == true {
            caption(w, p, "Review is truncated. Approval is disabled.");
        }
        if review["policy"]["violations"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        {
            caption(
                w,
                p,
                "Policy violations block approval here. Resolve them before approving.",
            );
        }
        visual::rule(w, p);
        button(
            w,
            p,
            s,
            if s.about {
                "About · hide evidence details"
            } else {
                "About · base, evidence and activity"
            },
            Action::About,
            !busy,
        );
        if s.about {
            if let Some(r) = revision(&review) {
                caption(
                    w,
                    p,
                    &format!("Base {} → {}", short(&r.baseline), short(&r.head)),
                );
                caption(w, p, &format!("Evidence {} · {}", r.evidence, r.digest));
            } else {
                caption(
                    w,
                    p,
                    "No complete sealed evidence. Decisions are unavailable.",
                );
            }
            for event in review["timeline"].as_array().into_iter().flatten().take(24) {
                caption(
                    w,
                    p,
                    &format!("{} · {}", string(event, "at"), string(event, "label")),
                );
            }
        }
    }
    let comments = review["comments"].as_array().cloned().unwrap_or_default();
    if !comments.is_empty() {
        heading(w, p, "Review comments");
    }
    for c in comments {
        if s.file
            .as_ref()
            .is_some_and(|f| c["path"].as_str() != Some(&f.0))
        {
            continue;
        }
        caption(
            w,
            p,
            &format!(
                "{} · {}:{} ({}){}",
                string(&c, "actor_id"),
                string(&c, "path"),
                c["start_line"],
                string(&c, "side"),
                if c["resolved_at"].is_null() {
                    ""
                } else {
                    " · resolved"
                }
            ),
        );
        caption(w, p, &string(&c, "body"));
        if c["resolved_at"].is_null() {
            button(
                w,
                p,
                s,
                "Resolve thread",
                Action::Resolve(string(&c, "id")),
                usable,
            );
        }
    }
    if s.confirmation.is_none()
        && !allowed(&review, "review")
        && !allowed(&review, "apply")
        && s.file.is_none()
    {
        return;
    }
    visual::rule(w, form);
    if let Some(m) = s.confirmation.clone() {
        heading(
            w,
            form,
            match m {
                Mutation::Approve { .. } => "Confirm approval of this evidence?",
                Mutation::Apply(_) => "Apply this approved decision?",
                Mutation::Request(_) => "Request changes to this evidence?",
                Mutation::Comment { .. } => "Post this anchored comment?",
                Mutation::Resolve(_) => "Resolve this comment?",
            },
        );
        match &m {
            Mutation::Approve {
                strategy,
                rationale,
            } => caption(w, form, &format!("Strategy: {strategy}. {rationale}")),
            Mutation::Comment {
                path,
                side,
                line,
                body,
                ..
            } => caption(w, form, &format!("{path} · {side}:{line} · {body}")),
            Mutation::Request(body) => caption(w, form, body),
            _ => {}
        }
        let bar = row(w, form);
        button(w, bar, s, "Confirm", Action::Confirm, usable);
        button(w, bar, s, "Cancel", Action::Cancel, !busy);
    } else {
        if let Some((path, _)) = &s.file {
            caption(w, form, &format!("Comment on {path}"));
        }
        s.body = Some(prompt(
            w,
            form,
            "Rationale or review comment",
            &s.draft.body,
            54.,
        ));
        let bar = row(w, form);
        if s.file.is_some() {
            s.line = Some(prompt(w, bar, "Line number", &s.draft.line, 32.));
            w.get_mut::<Layout>(s.line.unwrap()).unwrap().width = Size::Px(100.);
            button(
                w,
                bar,
                s,
                if s.draft.old { "Old side" } else { "New side" },
                Action::Side,
                !busy,
            );
            button(
                w,
                bar,
                s,
                "Add comment",
                Action::Comment,
                usable && s.file.as_ref().is_some_and(|(_, v)| !v.is_null()),
            );
        }
        button(
            w,
            bar,
            s,
            if s.strategy {
                "Apply to starting branch"
            } else {
                "Keep reviewed branch"
            },
            Action::Strategy,
            !busy,
        );
        button(
            w,
            bar,
            s,
            "Approve",
            Action::Approve,
            usable && approvable(&review),
        );
        button(
            w,
            bar,
            s,
            "Request changes",
            Action::Request,
            usable && allowed(&review, "review"),
        );
        if allowed(&review, "apply") {
            button(
                w,
                bar,
                s,
                "Apply approved decision",
                Action::Apply,
                usable && !string(&review["decision"], "id").is_empty(),
            );
        }
        if !allowed(&review, "review") {
            caption(
                w,
                form,
                &string(&review["allowed_actions"]["review"], "reason"),
            );
        }
        if busy {
            caption(w, form, "Submitting once · no automatic retries");
        }
        if s.uncertain {
            caption(w, form, "Refresh required before any further decision.");
        }
    }
}
fn short(s: &str) -> &str {
    s.get(..s.len().min(12)).unwrap_or(s)
}
