use super::*;
use crate::native_style as visual;
use rndr_ui::{
    Token, Typography, UiId,
    controls::{self, ButtonProps, NavigationMeta},
    text,
};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone)]
struct FileDiff(String, Revision, bool);
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
/// Label in an explicit type role and tone (an app UiStyle outranks recipes).
fn styled(w: &mut World, p: Entity, s: &str, typography: Typography, tone: Token) -> Entity {
    let e = text(w, p, s, typography).unwrap();
    w.insert(
        e,
        rndr_ui::UiStyle {
            typography: Some(typography),
            foreground: Some(tone.into()),
            ..Default::default()
        },
    );
    e
}
/// Measured width, so short metadata never ellipsizes beside a filling label.
fn fitted(w: &mut World, e: Entity, max: f32) {
    w.get_mut::<Layout>(e).unwrap().width = Size::Hug;
    w.insert(
        e,
        visual::FitLabel {
            label: e,
            extra: 0.,
            min: 0.,
            max,
        },
    );
}
/// One non-wrapping line, children centered on the cross axis.
fn line(w: &mut World, p: Entity, gap: f32) -> Entity {
    let e = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap,
            ..Layout::row()
        },
    );
    w.insert(
        e,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    e
}
/// Keeps trailing metadata clear of the overlay scrollbar.
fn gutter(w: &mut World, p: Entity) {
    panel(
        w,
        p,
        Layout {
            width: Size::Px(12.),
            height: Size::Px(1.),
            ..Layout::row()
        },
    );
}
/// Icon glyph in its own face: the shared icon recipe centres a glyph's
/// parent, which must not be the surrounding line.
fn glyph(w: &mut World, p: Entity, icon: Icon, tone: Token) -> Entity {
    let face = panel(
        w,
        p,
        Layout {
            width: Size::Px(visual::ACTION_ICON),
            height: Size::Px(visual::ACTION_ICON),
            ..Layout::row()
        },
    );
    styled(w, face, &icon.scalar().to_string(), Typography::Label, tone);
    face
}
/// medousa-home's path-based change kinds, in display order.
const KINDS: [(&str, Icon); 5] = [
    ("Code", Icon::Code),
    ("Tests", Icon::CheckCheck),
    ("CI", Icon::Settings),
    ("Docs", Icon::Book),
    ("Other", Icon::File),
];
/// Index into [`KINDS`]; precedence matches `reviewChangeKind` in medousa-home.
pub(super) fn change_kind(path: &str) -> usize {
    let p = path.replace('\\', "/").to_lowercase();
    let (dirs, name) = p.rsplit_once('/').unwrap_or(("", &p));
    let dirs: Vec<&str> = dirs.split('/').filter(|d| !d.is_empty()).collect();
    let in_dir = |names: &[&str]| dirs.iter().any(|d| names.contains(d));
    let (base, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let stem = name.split('.').next().unwrap_or(name);
    if in_dir(&[".circleci", ".buildkite", "ci"])
        || p.starts_with(".github/workflows/")
        || p.starts_with(".github/actions/")
        || p.contains("/.github/workflows/")
        || p.contains("/.github/actions/")
        || name.starts_with(".gitlab-ci")
        || name.starts_with("azure-pipelines")
        || name == "jenkinsfile"
        || name == ".travis.yml"
    {
        return 2;
    }
    if in_dir(&[
        "test",
        "tests",
        "__tests__",
        "spec",
        "specs",
        "fixtures",
        "__snapshots__",
    ]) || ["test", "tests", "spec", "specs"].contains(&base)
        || name.starts_with("test_")
        || ["test", "tests", "spec", "specs"]
            .iter()
            .any(|t| base.ends_with(&format!(".{t}")) || base.ends_with(&format!("_{t}")))
    {
        return 1;
    }
    if in_dir(&["doc", "docs", "documentation"])
        || ["md", "mdx", "rst", "adoc"].contains(&ext)
        || ["readme", "changelog", "license", "contributing"].contains(&stem)
    {
        return 3;
    }
    const CODE: &[&str] = &[
        "rs", "js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts", "svelte", "vue", "py", "go",
        "java", "kt", "swift", "c", "h", "cpp", "hpp", "cxx", "hxx", "cs", "rb", "php", "sh",
        "bash", "zsh", "sql", "graphql", "gr", "css", "scss", "sass", "less", "html", "ex", "exs",
        "erl", "clj", "dart", "lua", "r", "m", "mm", "zig", "wasm",
    ];
    if CODE.contains(&ext) { 0 } else { 4 }
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
/// medousa-home's file row: chevron, file glyph, bold name, quiet folder,
/// then coloured line counts. One navigation control toggles the diff.
fn file_header(w: &mut World, p: Entity, s: &mut State, file: &Value, open: bool) {
    let path = string(file, "path");
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", &path));
    let id = next();
    let mut props = ButtonProps::new(name, HostIntent::Command(id));
    props.enabled = s.mutation_pending.is_none();
    let h =
        controls::NavigationRow::mount(w, p, UiId(id), props, NavigationMeta::default()).unwrap();
    s.actions.insert(id, Action::ToggleFile(path.clone()));
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(36.),
        ..Layout::column()
    };
    w.insert(
        h.root,
        Semantics {
            name: Some(format!(
                "{} {path}",
                if open { "Collapse" } else { "Expand" }
            )),
            ..Default::default()
        },
    );
    let Some(face) = w.get::<Parent>(h.label).map(|p| p.entity) else {
        return;
    };
    if let Some(l) = w.get_mut::<Layout>(face) {
        l.axis = Axis::Row;
        l.gap = 6.;
    }
    w.insert(
        face,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    w.insert(
        h.label,
        rndr_ui::UiStyle {
            typography: Some(Typography::Emphasis),
            foreground: Some(Token::Text.into()),
            ..Default::default()
        },
    );
    fitted(w, h.label, 360.);
    let chevron = glyph(
        w,
        face,
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        Token::Tertiary,
    );
    let icon = glyph(w, face, Icon::FileCode, Token::Tertiary);
    // Leading glyphs go before the label; RNDR appends children.
    if let Some(children) = w.get_mut::<Children>(face) {
        children.list.retain(|e| *e != chevron && *e != icon);
        children.list.splice(0..0, [chevron, icon]);
    }
    let folder = styled(w, face, dir, Typography::Caption, Token::Secondary);
    w.get_mut::<Layout>(folder).unwrap().width = Size::Fill;
    w.get_mut::<Label>(folder).unwrap().overflow = TextOverflow::Ellipsis;
    let added = file["lines_added"].as_u64().unwrap_or(0);
    let removed = file["lines_removed"].as_u64().unwrap_or(0);
    if added > 0 || removed > 0 {
        for (value, tone) in [
            (format!("+{added}"), Token::SuccessText),
            (format!("−{removed}"), Token::DangerText),
        ] {
            let e = styled(w, face, &value, Typography::Caption, tone);
            fitted(w, e, 80.);
        }
    }
}
/// "No checks ran" is already the Checks panel's state, not an issue.
fn routine_issue(issue: &str) -> bool {
    let issue = issue.to_lowercase();
    issue.contains("no project check") || issue.contains("project checks haven")
}
/// medousa-home's short symbol kinds.
fn short_kind(kind: &str) -> &'static str {
    let raw = kind.to_lowercase();
    if ["fn", "function", "method"].iter().any(|k| raw.contains(k)) {
        "fn"
    } else if ["struct", "class", "type"].iter().any(|k| raw.contains(k)) {
        "type"
    } else if ["trait", "interface"].iter().any(|k| raw.contains(k)) {
        "trait"
    } else if raw.contains("mod") {
        "mod"
    } else if raw.contains("enum") {
        "enum"
    } else if raw.contains("const") || raw.contains("static") {
        "const"
    } else {
        "sym"
    }
}
/// Collapsible "Changed symbols · N" list from the daemon's per-file scopes.
fn symbols(w: &mut World, p: Entity, s: &mut State, file: &Value, path: &str) {
    let scopes = file["scopes"].as_array().cloned().unwrap_or_default();
    if scopes.is_empty() {
        return;
    }
    let open = s.symbols.contains(path);
    let id = next();
    let label = format!(
        "{}  Changed symbols · {}",
        if open {
            Icon::ChevronDown.scalar()
        } else {
            Icon::ChevronRight.scalar()
        },
        scopes.len()
    );
    let h = visual::document_action(w, p, id, &label, true);
    w.insert(
        h.label,
        rndr_ui::UiStyle {
            typography: Some(Typography::Caption),
            foreground: Some(Token::Secondary.into()),
            ..Default::default()
        },
    );
    s.actions.insert(id, Action::Symbols(path.to_owned()));
    if !open {
        return;
    }
    let list = column(w, p);
    w.get_mut::<Layout>(list).unwrap().gap = 2.;
    for scope in &scopes {
        let row = line(w, list, 8.);
        w.get_mut::<Layout>(row).unwrap().height = Size::Px(24.);
        gutter(w, row);
        let kind = styled(
            w,
            row,
            short_kind(&string(scope, "kind")),
            Typography::Code,
            Token::Tertiary,
        );
        w.get_mut::<Layout>(kind).unwrap().width = Size::Px(40.);
        let full = string(scope, "label");
        let name = full.rsplit("::").next().unwrap_or(&full).trim().to_owned();
        let e = styled(
            w,
            row,
            if name.is_empty() { &full } else { &name },
            Typography::Secondary,
            Token::Text,
        );
        w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
        w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Ellipsis;
        for (value, tone) in [
            (
                format!("+{}", scope["lines_added"].as_u64().unwrap_or(0)),
                Token::SuccessText,
            ),
            (
                format!("−{}", scope["lines_removed"].as_u64().unwrap_or(0)),
                Token::DangerText,
            ),
        ] {
            let e = styled(w, row, &value, Typography::Caption, tone);
            fitted(w, e, 80.);
        }
        gutter(w, row);
    }
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
        if revision(&s.review).as_ref() != Some(&file.1) || s.context.contains(&file.0) != file.2 {
            continue;
        }
        let handle = rndr_ui::collections::VirtualViewHandle { root };
        if let (Some(anchor), Some(scroll)) = (handle.anchor(w), w.get::<Scroll>(root)) {
            s.diff_positions
                .insert(file.0.clone(), (anchor, scroll.offset[0]));
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
    let tabs = line(w, p, 6.);
    w.get_mut::<Align>(tabs).unwrap().items = Items::End;
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
    panel(
        w,
        tabs,
        Layout {
            width: Size::Fill,
            height: Size::Px(1.),
            ..Layout::row()
        },
    );
    let executor = review["candidates"]
        .as_array()
        .and_then(|a| a.iter().find(|c| c["attempt_id"] == review["attempt_id"]))
        .map(|c| string(c, "executor"))
        .unwrap_or_default();
    let e = styled(
        w,
        tabs,
        &format!(
            "Revision {}{}{}",
            review["attempt_seq"],
            if executor.is_empty() { "" } else { " · " },
            executor
        ),
        Typography::Caption,
        Token::Secondary,
    );
    fitted(w, e, 320.);
    gutter(w, tabs);
    visual::rule(w, p);
    if s.changes {
        let tools = line(w, p, 4.);
        let count = styled(
            w,
            tools,
            &format!(
                "{} changed file{}",
                files.len(),
                if files.len() == 1 { "" } else { "s" }
            ),
            Typography::Caption,
            Token::Tertiary,
        );
        w.get_mut::<Layout>(count).unwrap().width = Size::Fill;
        button(
            w,
            tools,
            s,
            "Expand all",
            Action::ExpandAll,
            !busy && s.read_pending.is_none(),
        );
        button(w, tools, s, "Collapse all", Action::CollapseAll, !busy);
        gutter(w, tools);
        // One full-width stream. File headers remain ordinary navigation controls
        // (keyboard/semantics included); each bounded diff owns its two-axis port.
        let list = column(w, p);
        w.get_mut::<Layout>(list).unwrap().gap = 0.;
        for file in &files {
            let path = string(file, "path");
            let open = s.expanded.contains(&path);
            let section = column(w, list);
            w.get_mut::<Layout>(section).unwrap().gap = if open { 8. } else { 0. };
            visual::rule(w, section);
            let header = line(w, section, 2.);
            file_header(w, header, s, file, open);
            for (icon, label, action, enabled) in [
                (
                    Icon::FileCode,
                    "Open in Code",
                    Action::OpenInCode(path.clone()),
                    !busy,
                ),
                (
                    Icon::Undo2,
                    "Restore before this change…",
                    Action::Restore(path.clone()),
                    usable && s.file_cache.contains_key(&path),
                ),
            ] {
                let id = next();
                visual::icon_action(w, header, id, icon, label, enabled);
                s.actions.insert(id, action);
            }
            gutter(w, header);
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
                    symbols(w, section, s, file, &path);
                    // Build the bounded sealed context once per revision/file,
                    // not on every form edit or accordion redraw.
                    let available = s
                        .context_cache
                        .entry(path.clone())
                        .or_insert_with(|| crate::diff_presentation::expanded_context(&value))
                        .is_some();
                    if available {
                        button(
                            w,
                            section,
                            s,
                            if s.context.contains(&path) {
                                "Collapse unchanged context"
                            } else {
                                "Show unchanged context"
                            },
                            Action::Context(path.clone()),
                            !busy,
                        );
                    }
                    let expanded = if s.context.contains(&path) {
                        s.context_cache.get(&path).and_then(|v| v.clone())
                    } else {
                        None
                    };
                    let value = expanded.as_ref().unwrap_or(&value);
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
                            value,
                        )
                        .unwrap();
                        w.insert(
                            handle.root,
                            FileDiff(
                                path.clone(),
                                revision(&review).unwrap(),
                                s.context.contains(&path),
                            ),
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
        w.get_mut::<Layout>(article).unwrap().gap = 14.;
        let title = text(w, article, &string(&review, "title"), Typography::Title).unwrap();
        w.get_mut::<Layout>(title).unwrap().width = Size::Fill;
        w.get_mut::<Label>(title).unwrap().overflow = TextOverflow::Wrap;
        let candidate = review["candidates"]
            .as_array()
            .and_then(|a| a.iter().find(|c| c["attempt_id"] == review["attempt_id"]));
        let author = candidate.map(|c| string(c, "executor")).unwrap_or_default();
        let byline = line(w, article, 10.);
        w.insert(byline, Wrap { line_gap: 6. });
        if !author.is_empty() {
            glyph(w, byline, Icon::User, Token::Tertiary);
            let e = styled(w, byline, &author, Typography::Caption, Token::Secondary);
            fitted(w, e, 240.);
        }
        let e = styled(
            w,
            byline,
            &format!("Revision {}", review["attempt_seq"]),
            Typography::Caption,
            Token::Secondary,
        );
        fitted(w, e, 120.);
        if let Some(branch) = candidate
            .map(|c| string(c, "branch"))
            .filter(|b| !b.is_empty())
        {
            let chip = panel(
                w,
                byline,
                Layout {
                    width: Size::Hug,
                    height: Size::Hug,
                    padding: 4.,
                    ..Layout::row()
                },
            );
            visual::plane(w, chip, visual::raised());
            let e = styled(w, chip, &branch, Typography::Code, Token::Secondary);
            fitted(w, e, 320.);
            w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Ellipsis;
        }
        let outcome = string(synthesis, "outcome");
        if !outcome.trim().is_empty() && outcome.trim() != string(&review, "title").trim() {
            let section = column(w, article);
            w.get_mut::<Layout>(section).unwrap().gap = 6.;
            heading(w, section, "Summary");
            reading(w, section, outcome.trim());
        }
        let attention: Vec<String> = synthesis["unresolved_issues"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|i| !routine_issue(i))
            .map(str::to_owned)
            .chain(
                synthesis["issues"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|issue| !routine_issue(&string(issue, "message")))
                    .map(|issue| {
                        format!(
                            "{}{}",
                            if issue["blocks_approval"] == true {
                                "Blocks approval · "
                            } else {
                                ""
                            },
                            string(issue, "message")
                        )
                    }),
            )
            .collect();
        if !attention.is_empty() {
            let section = column(w, article);
            w.get_mut::<Layout>(section).unwrap().gap = 6.;
            let head = line(w, section, 6.);
            glyph(w, head, Icon::CircleAlert, Token::WarningText);
            let e = styled(
                w,
                head,
                "Needs attention",
                Typography::Emphasis,
                Token::WarningText,
            );
            w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
            for issue in &attention {
                reading(w, section, &format!("•  {issue}"));
            }
        }
        let areas_section = column(w, article);
        w.get_mut::<Layout>(areas_section).unwrap().gap = 0.;
        heading(w, areas_section, "Changed areas");
        let mut areas = std::collections::BTreeMap::<String, usize>::new();
        for file in &files {
            let path = string(file, "path");
            let area = path
                .split_once('/')
                .map_or(String::new(), |(a, _)| a.into());
            *areas.entry(area).or_default() += 1;
        }
        let mut areas: Vec<_> = areas.into_iter().collect();
        areas.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        if areas.is_empty() {
            caption(w, areas_section, "No file changes in this revision.");
        }
        for (directory, count) in areas {
            let label = if directory.is_empty() {
                "Repository root".to_owned()
            } else {
                format!("{directory}/")
            };
            let area_row = line(w, areas_section, 12.);
            w.get_mut::<Layout>(area_row).unwrap().padding = 10.;
            let copy = panel(
                w,
                area_row,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 3.,
                    ..Layout::column()
                },
            );
            let name = styled(w, copy, &label, Typography::Label, Token::Text);
            w.get_mut::<Layout>(name).unwrap().width = Size::Fill;
            let names: Vec<_> = files
                .iter()
                .filter_map(|f| f["path"].as_str())
                .filter_map(|path| {
                    if directory.is_empty() {
                        (!path.contains('/')).then_some(path)
                    } else {
                        path.strip_prefix(&format!("{directory}/"))
                    }
                })
                .take(3)
                .collect();
            let mut preview = names.join(", ");
            if count > names.len() {
                preview.push_str(&format!("  +{} more", count - names.len()));
            }
            let e = styled(w, copy, &preview, Typography::Caption, Token::Secondary);
            w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
            w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Ellipsis;
            let e = styled(
                w,
                area_row,
                &format!("{count} {}", if count == 1 { "file" } else { "files" }),
                Typography::Caption,
                Token::Secondary,
            );
            fitted(w, e, 80.);
            let id = next();
            visual::icon_action(
                w,
                area_row,
                id,
                Icon::ArrowUpRight,
                &format!("Browse {label} changes"),
                !busy,
            );
            s.actions.insert(id, Action::Changes);
            visual::rule(w, areas_section);
        }
        let status = string(synthesis, "status_summary");
        let next_action = string(synthesis, "recommended_next_action");
        if !status.is_empty() || !next_action.is_empty() {
            let section = column(w, article);
            w.get_mut::<Layout>(section).unwrap().gap = 6.;
            heading(w, section, "Review status");
            reading(w, section, &status);
            caption(w, section, &next_action);
        }
        if let Some(rationale) = review["decision"]["rationale"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            let section = column(w, article);
            w.get_mut::<Layout>(section).unwrap().gap = 6.;
            heading(w, section, "Review decision");
            reading(w, section, rationale);
        }
        let meta = column(w, content);
        w.get_mut::<Layout>(meta).unwrap().gap = 14.;
        if wide {
            w.get_mut::<Layout>(meta).unwrap().width = Size::Px(240.);
        }
        // Changes by kind, as medousa-home's typed breakdown.
        let changes = column(w, meta);
        w.get_mut::<Layout>(changes).unwrap().gap = 4.;
        let head = line(w, changes, 8.);
        let e = styled(w, head, "Changes", Typography::Caption, Token::Tertiary);
        w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
        let e = styled(
            w,
            head,
            &format!(
                "{} {}",
                files.len(),
                if files.len() == 1 { "file" } else { "files" }
            ),
            Typography::Caption,
            Token::Tertiary,
        );
        fitted(w, e, 90.);
        let mut kinds = [(0usize, 0u64, 0u64); 5];
        for file in &files {
            let kind = &mut kinds[change_kind(&string(file, "path"))];
            kind.0 += 1;
            kind.1 += file["lines_added"].as_u64().unwrap_or(0);
            kind.2 += file["lines_removed"].as_u64().unwrap_or(0);
        }
        for ((label, icon), (count, added, removed)) in KINDS.iter().zip(kinds) {
            if count == 0 {
                continue;
            }
            let kind_row = line(w, changes, 8.);
            w.get_mut::<Layout>(kind_row).unwrap().height = Size::Px(26.);
            glyph(w, kind_row, *icon, Token::Tertiary);
            let e = styled(w, kind_row, label, Typography::Secondary, Token::Text);
            w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
            for (value, tone) in [
                (count.to_string(), Token::Tertiary),
                (format!("+{added}"), Token::SuccessText),
                (format!("−{removed}"), Token::DangerText),
            ] {
                let e = styled(w, kind_row, &value, Typography::Caption, tone);
                fitted(w, e, 80.);
            }
        }
        visual::rule(w, meta);
        let checks = column(w, meta);
        w.get_mut::<Layout>(checks).unwrap().gap = 6.;
        styled(w, checks, "Checks", Typography::Caption, Token::Tertiary);
        let v = &synthesis["verification"];
        let (icon, tone, state) = if !v.is_object() {
            (Icon::Circle, Token::Secondary, "No checks recorded")
        } else if v["success"] == true {
            (Icon::CircleCheck, Token::SuccessText, "Check passed")
        } else {
            (Icon::CircleAlert, Token::DangerText, "Check failed")
        };
        let status_row = line(w, checks, 6.);
        glyph(w, status_row, icon, tone);
        let e = styled(w, status_row, state, Typography::Secondary, tone);
        w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
        if v.is_object() {
            caption(
                w,
                checks,
                &format!(
                    "{}{}",
                    string(v, "label"),
                    v["exit_code"]
                        .as_i64()
                        .map(|c| format!(" · exit {c}"))
                        .unwrap_or_default()
                ),
            );
            let command = v["command"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            if !command.is_empty() {
                let e = styled(w, checks, &command, Typography::Code, Token::Secondary);
                w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
                w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
            }
        }
        let risk = string(synthesis, "risk");
        if !risk.is_empty() {
            visual::rule(w, meta);
            let section = column(w, meta);
            w.get_mut::<Layout>(section).unwrap().gap = 6.;
            styled(w, section, "Risk", Typography::Caption, Token::Tertiary);
            let (label, tone) = match risk.as_str() {
                "high" => ("High".to_owned(), Token::DangerText),
                "attention" => ("Needs attention".to_owned(), Token::WarningText),
                "medium" => ("Medium".to_owned(), Token::WarningText),
                other => {
                    let mut chars = other.chars();
                    let first = chars.next().map(|c| c.to_uppercase().collect::<String>());
                    (first.unwrap_or_default() + chars.as_str(), Token::Text)
                }
            };
            let e = styled(w, section, &label, Typography::Secondary, tone);
            w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
            let summary = string(synthesis, "risk_summary");
            if !summary.is_empty() {
                caption(w, section, &summary);
            }
        }
        gutter(w, content);
        let candidates = review["candidates"].as_array().cloned().unwrap_or_default();
        if candidates.len() > 1 {
            visual::rule(w, meta);
            let section = column(w, meta);
            w.get_mut::<Layout>(section).unwrap().gap = 2.;
            styled(
                w,
                section,
                "Revisions",
                Typography::Caption,
                Token::Tertiary,
            );
            for c in candidates {
                button(
                    w,
                    section,
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
                Mutation::Restore { .. } => "Restore this file to before this change?",
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
            Mutation::Restore { path, .. } => caption(
                w,
                form,
                &format!(
                    "{path} goes back to its starting version in the reviewed branch, then opens in Code."
                ),
            ),
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
