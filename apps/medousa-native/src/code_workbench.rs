//! Explicit daemon-backed workbench commands. Forms and results are retained
//! independently of editor redraws; command scope never follows a later tab.
use super::*;
use rndr_core::editing::SourceSelection;

#[derive(Clone, Debug)]
pub(super) struct Command {
    pub work: String,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    Search(SearchOptions),
    Changes,
    Diff(String),
    History,
    Tasks,
    Tests,
    Runs,
    Run {
        task: String,
        test: Option<String>,
        lease: Lease,
    },
    Output(String),
    Cancel(String),
    Language {
        path: String,
        digest: String,
        feature: Feature,
        line: usize,
        character: usize,
    },
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Feature {
    Diagnostics,
    Symbols,
    Hover,
    Definition,
    References,
}
#[derive(Clone)]
pub(super) enum Action {
    QuickOpen,
    FilterFiles,
    Search,
    SubmitSearch,
    SearchToggle(SearchToggle),
    MoreSearch,
    Changes,
    Diff(String),
    History,
    Tasks,
    Runs,
    Run(String),
    Tests,
    RunTest {
        task: String,
        test: String,
    },
    Output(String),
    Cancel(String),
    Close,
    Language(Feature),
    Open {
        work: String,
        path: String,
        line: usize,
        character: usize,
    },
}
#[derive(Clone, Copy)]
pub(super) enum SearchToggle {
    Regex,
    Case,
    Word,
    Changed,
    Ignored,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SearchOptions {
    query: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    changed: bool,
    ignored: bool,
    include: String,
    exclude: String,
    cursor: Option<String>,
}
impl SearchOptions {
    fn parameters(&self) -> Vec<(&'static str, String)> {
        let mut parameters = vec![
            ("query", self.query.clone()),
            ("mode", if self.regex { "regex" } else { "literal" }.into()),
            ("case_sensitive", self.case_sensitive.to_string()),
            ("whole_word", self.whole_word.to_string()),
            ("include", self.include.clone()),
            ("exclude", self.exclude.clone()),
            ("include_ignored", self.ignored.to_string()),
            ("scope", if self.changed { "changed" } else { "all" }.into()),
            ("limit", "100".into()),
        ];
        if let Some(cursor) = &self.cursor {
            parameters.push(("cursor", cursor.clone()));
        }
        parameters
    }
}
#[derive(Default)]
pub(super) struct Workbench {
    panel: Option<Entity>,
    input: Option<Entity>,
    scope: Option<String>,
    pub actions: HashMap<u64, super::Action>,
    run_uncertain: HashSet<String>,
    jump: Option<(Key, usize, usize)>,
    searches: HashMap<String, SearchOptions>,
    search_fields: Option<[Entity; 3]>,
    next_search: Option<SearchOptions>,
}
impl Workbench {
    pub fn new(panel: Entity) -> Self {
        Self {
            panel: Some(panel),
            ..Default::default()
        }
    }
}
fn busy(c: &Code, work: &str) -> bool {
    c.pending.contains_key(&format!("workbench:{work}"))
}
pub(super) fn navigation(w: &mut World, p: Entity, c: &mut Code) {
    let enabled = c.selected.as_ref().is_some_and(|s| !busy(c, s));
    for (label, action) in [
        ("Open file", Action::QuickOpen),
        ("Search", Action::Search),
        ("Changes", Action::Changes),
        ("Tasks", Action::Tasks),
        ("Tests", Action::Tests),
        ("Run history", Action::Runs),
    ] {
        button(w, p, c, label, super::Action::Workbench(action), enabled);
    }
    let p = row(w, c.toolbar.unwrap());
    let clean = c
        .key()
        .and_then(|k| c.buffers.get(&k))
        .is_some_and(|b| !b.dirty() && !b.uncertain && b.remote.is_none() && !b.baseline.readonly);
    for (label, feature) in [
        ("Problems", Feature::Diagnostics),
        ("Outline", Feature::Symbols),
        ("Hover", Feature::Hover),
        ("Definition", Feature::Definition),
        ("Find uses", Feature::References),
    ] {
        button(
            w,
            p,
            c,
            label,
            super::Action::Workbench(Action::Language(feature)),
            enabled && clean,
        );
    }
    // Hide results from another project, but keep the form/draft retained.
    if let Some(panel) = c.workbench.panel {
        if c.workbench.scope.as_ref() != c.selected.as_ref() {
            w.insert(panel, Hidden);
        }
    }
}
fn control(w: &mut World, p: Entity, c: &mut Code, label: &str, action: Action, enabled: bool) {
    let id = next();
    visual::document_action(w, p, id, label, enabled);
    c.workbench
        .actions
        .insert(id, super::Action::Workbench(action));
}
fn panel(w: &mut World, c: &mut Code, work: &str, title: &str) -> Option<Entity> {
    let p = c.workbench.panel?;
    if let Some(f) = w.resource_mut::<Focus>() {
        f.entity = None;
    }
    capture_search(w, c);
    for e in children_of(w, p) {
        despawn(w, e);
    }
    w.remove::<Hidden>(p);
    c.workbench.input = None;
    c.workbench.search_fields = None;
    c.workbench.next_search = None;
    c.workbench.scope = Some(work.into());
    c.workbench.actions.clear();
    caption(w, p, title);
    control(w, p, c, "Close panel", Action::Close, true);
    Some(p)
}
fn capture_search(w: &World, c: &mut Code) {
    if let (Some(work), Some(fields)) = (&c.workbench.scope, c.workbench.search_fields) {
        let state = c.workbench.searches.entry(work.clone()).or_default();
        state.query = w
            .get::<Label>(fields[0])
            .map(|l| l.text.clone())
            .unwrap_or_default();
        state.include = w
            .get::<Label>(fields[1])
            .map(|l| l.text.clone())
            .unwrap_or_default();
        state.exclude = w
            .get::<Label>(fields[2])
            .map(|l| l.text.clone())
            .unwrap_or_default();
        state.cursor = None;
    }
}
fn search_form(w: &mut World, c: &mut Code, work: &str) {
    capture_search(w, c);
    let state = c.workbench.searches.entry(work.into()).or_default().clone();
    let Some(p) = panel(
        w,
        c,
        work,
        "Search saved project files · at most 100 hits per page",
    ) else {
        return;
    };
    let fields = [
        ("Search project contents", &state.query),
        ("Include globs (comma-separated)", &state.include),
        ("Exclude globs (comma-separated)", &state.exclude),
    ]
    .map(|(name, value)| {
        let e = w.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(32.),
                ..Layout::row()
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
        e
    });
    c.workbench.input = Some(fields[0]);
    c.workbench.search_fields = Some(fields);
    for (name, on, toggle) in [
        ("Regex", state.regex, SearchToggle::Regex),
        ("Match case", state.case_sensitive, SearchToggle::Case),
        ("Whole word", state.whole_word, SearchToggle::Word),
        ("Changed files only", state.changed, SearchToggle::Changed),
        ("Include ignored", state.ignored, SearchToggle::Ignored),
    ] {
        control(
            w,
            p,
            c,
            &format!("{} {name}", if on { "✓" } else { "○" }),
            Action::SearchToggle(toggle),
            true,
        );
    }
    control(w, p, c, "Search saved files", Action::SubmitSearch, true);
    if let Some(focus) = w.resource_mut::<Focus>() {
        focus.entity = Some(fields[0]);
    }
}
fn enqueue(c: &mut Code, work: String, kind: Kind) {
    c.enqueue(Operation::Workbench(Command { work, kind }));
}
fn clean_project(c: &Code, work: &str) -> bool {
    !c.buffers
        .iter()
        .any(|(key, b)| key.0 == work && (b.dirty() || b.uncertain || b.remote.is_some()))
        && !c
            .pending
            .keys()
            .any(|s| s.starts_with(&format!("save:{work}:")))
}
pub(super) fn action(w: &mut World, c: &mut Code, action: Action) {
    if composing(w, c) {
        return;
    }
    let Some(work) = c.selected.clone() else {
        return;
    };
    if matches!(action, Action::Close) {
        capture_search(w, c);
        if let Some(p) = c.workbench.panel {
            w.insert(p, Hidden);
        }
        c.workbench.actions.clear();
        return;
    }
    if busy(c, &work) {
        return;
    }
    // Persistent panel controls must not apply to a different selected project.
    let scoped = matches!(
        action,
        Action::SubmitSearch
            | Action::SearchToggle(_)
            | Action::MoreSearch
            | Action::FilterFiles
            | Action::Diff(_)
            | Action::History
            | Action::Run(_)
            | Action::RunTest { .. }
            | Action::Output(_)
            | Action::Cancel(_)
    );
    if scoped && c.workbench.scope.as_ref() != Some(&work) {
        return;
    }
    match action {
        Action::QuickOpen => {
            if let Some(p) = panel(w, c, &work, "Open project file · bounded daemon tree") {
                let input = w.spawn((
                    Layout {
                        width: Size::Fill,
                        height: Size::Px(32.),
                        ..Layout::row()
                    },
                    Label::new(""),
                    Focusable,
                    Prompt {
                        submit_on_enter: false,
                        ..Prompt::new()
                    },
                    Placeholder::new("Filter relative paths"),
                    Semantics {
                        name: Some("Filter project file paths".into()),
                        ..Default::default()
                    },
                ));
                set_parent(w, input, p);
                c.workbench.input = Some(input);
                control(w, p, c, "Find paths", Action::FilterFiles, true);
                if let Some(focus) = w.resource_mut::<Focus>() {
                    focus.entity = Some(input);
                }
            }
        }
        Action::FilterFiles => {
            let query = c
                .workbench
                .input
                .and_then(|e| w.get::<Label>(e))
                .map(|l| l.text.trim().to_lowercase())
                .unwrap_or_default();
            let matches: Vec<_> = c
                .files
                .get(&work)
                .into_iter()
                .flatten()
                .filter(|path| path.to_lowercase().contains(&query))
                .take(101)
                .cloned()
                .collect();
            if let Some(p) = panel(w, c, &work, "Matching project paths · first 100") {
                control(w, p, c, "Change path filter", Action::QuickOpen, true);
                if matches.is_empty() {
                    caption(
                        w,
                        p,
                        "No paths in the loaded tree. Refresh Explorer if files have changed.",
                    );
                }
                for path in matches.iter().take(100) {
                    open_control(w, p, c, &work, path, 0, 0, path);
                }
                if matches.len() > 100 || c.truncated.contains(&work) {
                    caption(
                        w,
                        p,
                        "Bounded file list · refine the path filter or refresh Explorer.",
                    );
                }
            }
        }
        Action::Search => search_form(w, c, &work),
        Action::SearchToggle(toggle) => {
            capture_search(w, c);
            let state = c.workbench.searches.entry(work.clone()).or_default();
            let field = match toggle {
                SearchToggle::Regex => &mut state.regex,
                SearchToggle::Case => &mut state.case_sensitive,
                SearchToggle::Word => &mut state.whole_word,
                SearchToggle::Changed => &mut state.changed,
                SearchToggle::Ignored => &mut state.ignored,
            };
            *field = !*field;
            search_form(w, c, &work);
        }
        Action::SubmitSearch => {
            capture_search(w, c);
            if let Some(state) = c
                .workbench
                .searches
                .get(&work)
                .filter(|s| !s.query.trim().is_empty())
                .cloned()
            {
                enqueue(c, work, Kind::Search(state));
            }
        }
        Action::MoreSearch => {
            if let Some(state) = c.workbench.next_search.clone() {
                enqueue(c, work, Kind::Search(state));
            }
        }
        Action::Changes => enqueue(c, work, Kind::Changes),
        Action::Diff(path) => enqueue(c, work, Kind::Diff(path)),
        Action::History => enqueue(c, work, Kind::History),
        Action::Tasks => enqueue(c, work, Kind::Tasks),
        Action::Tests => enqueue(c, work, Kind::Tests),
        Action::Runs => enqueue(c, work, Kind::Runs),
        Action::Run(_) | Action::RunTest { .. } => {
            let (task, test) = match action {
                Action::Run(task) => (task, None),
                Action::RunTest { task, test } => (task, Some(test)),
                _ => unreachable!(),
            };
            if !clean_project(c, &work) {
                panel(
                    w,
                    c,
                    &work,
                    "Save all project drafts and resolve uncertain saves before running a task.",
                );
                return;
            }
            // The shared busy(c, &work) guard above reads Code.pending;
            // admission uncertainty outlives that request's terminal reply.
            if c.workbench.run_uncertain.contains(&work) {
                return;
            }
            if let Some(s) = c
                .sessions
                .get(&work)
                .filter(|s| s.checked.elapsed() < Duration::from_secs(45))
            {
                let lease = s.lease.clone();
                // A timed-out start is not safe to replay. Listing runs is
                // observation only and cannot clear this admission uncertainty.
                c.workbench.run_uncertain.insert(work.clone());
                enqueue(c, work, Kind::Run { task, test, lease });
            } else {
                panel(
                    w,
                    c,
                    &work,
                    "Begin editing to obtain current task execution ownership.",
                );
            }
        }
        Action::Output(run) => enqueue(c, work, Kind::Output(run)),
        Action::Cancel(run) => enqueue(c, work, Kind::Cancel(run)),
        Action::Language(feature) => {
            let Some(key) = c.key() else {
                return;
            };
            let Some(b) = c.buffers.get(&key).filter(|b| {
                !b.dirty() && !b.uncertain && b.remote.is_none() && !b.baseline.readonly
            }) else {
                return;
            };
            let digest = b.baseline.digest.clone();
            let head = c
                .views
                .get(&key)
                .and_then(|v| {
                    w.resource::<SourceDocuments>()?.get(v.document).map(|d| {
                        d.view_selection(v.handle.editor)
                            .unwrap_or_else(|| d.selection())
                            .head
                    })
                })
                .unwrap_or(0);
            let (line, character) = position(&b.content, head);
            enqueue(
                c,
                work,
                Kind::Language {
                    path: key.1,
                    digest,
                    feature,
                    line,
                    character,
                },
            );
        }
        Action::Open {
            work: target,
            path,
            line,
            character,
        } => {
            if target != work || !safe_path(&path) {
                return;
            }
            let key = (work.clone(), path.clone());
            c.open_tab(&key);
            c.current.insert(work, path);
            c.workbench.jump = Some((key.clone(), line, character));
            if c.buffers.contains_key(&key) {
                apply_jump(w, c);
            } else {
                c.enqueue(Operation::Load(key));
            }
        }
        Action::Close => {}
    }
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0', ':'])
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
fn position(text: &str, head: usize) -> (usize, usize) {
    let mut end = head.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = &text[..end];
    (
        prefix.bytes().filter(|b| *b == b'\n').count(),
        prefix
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .encode_utf16()
            .count(),
    )
}
pub(super) fn apply_jump(w: &mut World, c: &mut Code) {
    let Some((key, line, character)) = c.workbench.jump.clone() else {
        return;
    };
    let Some(view) = c.views.get(&key) else {
        return;
    };
    let Some(d) = w
        .resource_mut::<SourceDocuments>()
        .and_then(|s| s.get_mut(view.document))
    else {
        return;
    };
    let mut offset = 0;
    for (n, segment) in d.text().split_inclusive('\n').enumerate() {
        if n == line {
            let mut units = 0;
            for ch in segment.chars().take_while(|ch| *ch != '\n') {
                if units + ch.len_utf16() > character {
                    break;
                }
                units += ch.len_utf16();
                offset += ch.len_utf8();
            }
            break;
        }
        offset += segment.len();
    }
    let _ = d.set_view_selection(
        view.handle.editor,
        d.revision(),
        SourceSelection {
            anchor: offset,
            head: offset,
        },
    );
    c.workbench.jump = None;
}
fn named_route(op: ops::Operation, work: &str, key: &str, value: &str) -> String {
    route(op, work).replace(key, &urlencoding::encode(value))
}
fn language(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "json" => "json",
        "svelte" => "svelte",
        "md" => "markdown",
        _ => "plaintext",
    }
}
fn file_uri(root: &str, path: &str) -> String {
    let absolute = format!(
        "{}/{}",
        root.trim_end_matches(['/', '\\']).replace('\\', "/"),
        path
    );
    let encoded = absolute
        .split('/')
        .map(|s| urlencoding::encode(s).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}
pub(super) async fn execute(
    client: &MedousaClient,
    command: &Command,
) -> Result<Value, medousa_sdk::SdkError> {
    let h = client.http();
    let work = &command.work;
    match &command.kind {
        Kind::Search(q) => {
            h.get_query(
                &route(ops::FORGE_ITEMS_BY_WORK_ID_SEARCH_GET, work),
                &q.parameters(),
            )
            .await
        }
        Kind::Changes => {
            h.get(&route(ops::FORGE_ITEMS_BY_WORK_ID_CHANGES_GET, work))
                .await
        }
        Kind::Diff(path) => {
            h.get_query(
                &route(ops::FORGE_ITEMS_BY_WORK_ID_CHANGES_FILE_GET, work),
                &[("path", path.clone())],
            )
            .await
        }
        Kind::History => {
            h.get(&route(
                ops::FORGE_ITEMS_BY_WORK_ID_CHANGES_HISTORY_GET,
                work,
            ))
            .await
        }
        Kind::Tasks => {
            h.get(&route(ops::FORGE_ITEMS_BY_WORK_ID_TASKS_GET, work))
                .await
        }
        Kind::Tests => {
            h.get(&route(ops::FORGE_ITEMS_BY_WORK_ID_TESTS_GET, work))
                .await
        }
        Kind::Runs => {
            h.get(&route(ops::FORGE_ITEMS_BY_WORK_ID_TASK_RUNS_GET, work))
                .await
        }
        Kind::Run { task, test, lease } => {
            h.post(
                &named_route(
                    ops::FORGE_ITEMS_BY_WORK_ID_TASKS_BY_TASK_ID_RUNS_POST,
                    work,
                    "{task_id}",
                    task,
                ),
                &json!({"lease_id":lease.id,"generation":lease.generation,"test_id":test}),
            )
            .await
        }
        Kind::Output(run) => {
            h.get(&named_route(
                ops::FORGE_ITEMS_BY_WORK_ID_TASK_RUNS_BY_RUN_ID_GET,
                work,
                "{run_id}",
                run,
            ))
            .await
        }
        Kind::Cancel(run) => {
            h.delete(&named_route(
                ops::FORGE_ITEMS_BY_WORK_ID_TASK_RUNS_BY_RUN_ID_DELETE,
                work,
                "{run_id}",
                run,
            ))
            .await
        }
        Kind::Language {
            path,
            feature,
            line,
            character,
            ..
        } => {
            let detail: Value = h.get(&route(ops::FORGE_ITEMS_BY_WORK_ID_GET, work)).await?;
            let root = detail["environment"]["worktree"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    medousa_sdk::SdkError::Serde("Project has no governed worktree".into())
                })?;
            if matches!(feature, Feature::References) {
                let result: Value = h
                    .post(
                        ops::CODE_REQUEST_POST.path,
                        &json!({
                            "action":"references", "work_id":work, "uri":file_uri(root, path),
                            "language":language(path), "line":line, "character":character,
                        }),
                    )
                    .await?;
                return Ok(json!({"root":root,"observation":result}));
            }
            let op = match feature {
                Feature::Diagnostics => ops::CODE_DIAGNOSTICS_GET,
                Feature::Symbols => ops::CODE_SYMBOLS_GET,
                Feature::Hover => ops::CODE_HOVER_GET,
                Feature::Definition => ops::CODE_DEFINITION_GET,
                Feature::References => unreachable!("references use code.request"),
            };
            let result: Value = h
                .get_query(
                    op.path,
                    &[
                        ("work_id", work.clone()),
                        ("uri", file_uri(root, path)),
                        ("language", language(path).into()),
                        ("line", line.to_string()),
                        ("character", character.to_string()),
                    ],
                )
                .await?;
            Ok(json!({"root":root,"observation":result}))
        }
    }
}
fn open_control(
    w: &mut World,
    p: Entity,
    c: &mut Code,
    work: &str,
    path: &str,
    line: usize,
    character: usize,
    label: &str,
) {
    if safe_path(path) {
        control(
            w,
            p,
            c,
            label,
            Action::Open {
                work: work.into(),
                path: path.into(),
                line,
                character,
            },
            true,
        );
    }
}
fn bounded(w: &mut World, p: Entity, text: &str) {
    // Results are explicitly bounded, and never mounted as editable documents.
    caption(w, p, &text.chars().take(16000).collect::<String>());
    if text.chars().count() > 16000 {
        caption(w, p, "Display truncated · refresh/open the file for more.");
    }
}
fn diff_rows(value: &Value) -> Vec<rndr_ui::diff::DiffRow> {
    use rndr_ui::diff::{DiffMarker, DiffRow, HunkId};
    let mut rows = Vec::new();
    if let Some(hunks) = value["hunks"].as_array() {
        for (index, hunk) in hunks.iter().enumerate() {
            let id = HunkId(index as u64 + 1);
            rows.push(DiffRow {
                id: rndr_ui::collections::ItemId(rows.len() as u64 + 1),
                hunk: id,
                text: format!(
                    "@@ -{},{} +{},{} @@",
                    hunk["old_start"], hunk["old_count"], hunk["new_start"], hunk["new_count"]
                ),
                marker: DiffMarker::Header,
                height: 24.,
            });
            if let Some(lines) = hunk["lines"].as_array() {
                for line in lines {
                    let marker = match line["kind"].as_str() {
                        Some("addition") => DiffMarker::Added,
                        Some("deletion") => DiffMarker::Removed,
                        _ => DiffMarker::Unchanged,
                    };
                    rows.push(DiffRow {
                        id: rndr_ui::collections::ItemId(rows.len() as u64 + 1),
                        hunk: id,
                        text: format!(
                            "{:>4} {:>4}  {}",
                            line["old_line"]
                                .as_u64()
                                .map(|n| n.to_string())
                                .unwrap_or_default(),
                            line["new_line"]
                                .as_u64()
                                .map(|n| n.to_string())
                                .unwrap_or_default(),
                            line["content"]
                                .as_str()
                                .unwrap_or("")
                                .trim_end_matches('\n')
                        ),
                        marker,
                        height: 22.,
                    });
                }
            }
        }
    }
    rows
}
fn result<'a>(value: &'a Value) -> &'a Value {
    value.get("result").unwrap_or(value)
}
fn location_path(uri: &str, root: &str) -> Option<String> {
    let encoded = uri.strip_prefix("file://")?;
    if encoded.to_ascii_lowercase().contains("%2f") || encoded.to_ascii_lowercase().contains("%5c")
    {
        return None;
    }
    let decoded = urlencoding::decode(encoded).ok()?;
    let normalized = root.trim_end_matches(['/', '\\']).replace('\\', "/");
    let absolute = if normalized.as_bytes().get(1) == Some(&b':') {
        decoded.trim_start_matches('/')
    } else {
        decoded.as_ref()
    };
    let path = absolute.strip_prefix(&format!("{normalized}/"))?;
    safe_path(path).then(|| path.into())
}
fn language_rows(
    w: &mut World,
    p: Entity,
    c: &mut Code,
    work: &str,
    path: &str,
    root: &str,
    rows: &Value,
    depth: usize,
) {
    if depth > 12 {
        return;
    }
    if let Some(items) = rows.as_array() {
        for item in items.iter().take(100) {
            let loc = item.get("location").unwrap_or(item);
            let target = loc["uri"]
                .as_str()
                .or_else(|| loc["targetUri"].as_str())
                .and_then(|uri| location_path(uri, root))
                .unwrap_or_else(|| path.into());
            // External definitions are displayed but never rebound to a local relative path.
            let external = loc["uri"]
                .as_str()
                .or_else(|| loc["targetUri"].as_str())
                .is_some_and(|uri| location_path(uri, root).is_none());
            let range = item
                .get("selectionRange")
                .or_else(|| loc.get("targetSelectionRange"))
                .or_else(|| loc.get("range"));
            let line = range.and_then(|r| r["start"]["line"].as_u64()).unwrap_or(0) as usize;
            let character = range
                .and_then(|r| r["start"]["character"].as_u64())
                .unwrap_or(0) as usize;
            let name = item["name"]
                .as_str()
                .or_else(|| item["message"].as_str())
                .unwrap_or(&target);
            let label = format!("{}{}:{} · {}", "  ".repeat(depth), target, line + 1, name);
            if external {
                caption(w, p, &label);
            } else {
                open_control(w, p, c, work, &target, line, character, &label);
            }
            if let Some(children) = item.get("children") {
                language_rows(w, p, c, work, path, root, children, depth + 1);
            }
        }
    } else if rows.is_object() {
        language_rows(
            w,
            p,
            c,
            work,
            path,
            root,
            &Value::Array(vec![rows.clone()]),
            depth,
        );
    }
}
pub(super) fn apply(
    w: &mut World,
    c: &mut Code,
    command: Command,
    response: Result<Value, String>,
) {
    let work = command.work;
    // Never project late responses onto another project; mutation uncertainty
    // remains scoped to its origin and can be inspected via run history.
    if c.selected.as_ref() != Some(&work) {
        return;
    }
    if let Kind::Language { path, digest, .. } = &command.kind {
        let key = (work.clone(), path.clone());
        if c.key().as_ref() != Some(&key)
            || !c.buffers.get(&key).is_some_and(|b| {
                !b.dirty() && !b.uncertain && b.remote.is_none() && b.baseline.digest == *digest
            })
        {
            return;
        }
    }
    let value = match response {
        Ok(v) => v,
        Err(e) => {
            if let Some(p) = panel(
                w,
                c,
                &work,
                &format!("Workshop operation not confirmed: {e}. No automatic retry."),
            ) {
                if matches!(command.kind, Kind::Run { .. } | Kind::Cancel(_)) {
                    control(
                        w,
                        p,
                        c,
                        "Inspect run history before retrying",
                        Action::Runs,
                        true,
                    );
                }
            }
            return;
        }
    };
    let Some(p) = panel(w, c, &work, "Project results · saved daemon state") else {
        return;
    };
    match command.kind {
        Kind::Search(mut options) => {
            if value["work_id"].as_str() != Some(&work) {
                caption(w, p, "Search identity mismatch");
                return;
            }
            if let Some(hits) = value["hits"].as_array() {
                for hit in hits.iter().take(100) {
                    if let Some(path) = hit["path"].as_str() {
                        let line = hit["line"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
                        open_control(
                            w,
                            p,
                            c,
                            &work,
                            path,
                            line,
                            0,
                            &format!(
                                "{}:{} · {}",
                                path,
                                line + 1,
                                hit["preview"].as_str().unwrap_or("")
                            ),
                        );
                    }
                }
                if hits.is_empty() {
                    caption(w, p, "No matches in saved files.");
                }
            }
            if value["truncated"].as_bool() == Some(true) {
                caption(w, p, "Results truncated · refine your query.");
            }
            if let Some(cursor) = value["next_cursor"].as_str().filter(|s| !s.is_empty()) {
                options.cursor = Some(cursor.into());
                c.workbench.next_search = Some(options);
                control(w, p, c, "Next results page", Action::MoreSearch, true);
            }
            control(w, p, c, "Edit search", Action::Search, true);
        }
        Kind::Changes => {
            if let Some(files) = value["files"].as_array() {
                for f in files.iter().take(200) {
                    if let Some(path) = f["path"].as_str() {
                        control(w, p, c, path, Action::Diff(path.into()), true);
                    }
                }
                if files.is_empty() {
                    caption(w, p, "No saved changes.");
                }
            } else {
                bounded(
                    w,
                    p,
                    &serde_json::to_string_pretty(&value).unwrap_or_default(),
                );
            }
            control(w, p, c, "History", Action::History, true);
        }
        Kind::Diff(path) => {
            if value["work_id"].as_str() != Some(&work) || value["path"].as_str() != Some(path.as_str()) {
                caption(w, p, "Diff identity mismatch · refresh Changes.");
            } else if value["binary"].as_bool() == Some(true) {
                caption(w, p, "Binary file · no text diff available.");
            } else {
                let rows = diff_rows(&value);
                let _ = rndr_ui::diff::DiffView::mount(
                    w,
                    p,
                    UiId(next()),
                    rndr_ui::diff::DiffProps::new(format!("Changes to {path}")),
                    &rows,
                );
                if value["truncated"].as_bool() == Some(true) {
                    caption(w, p, "Daemon diff truncated · not the complete change set.");
                }
                if value["conflict"].as_bool() == Some(true) {
                    caption(w, p, "Git conflict · resolve explicitly before saving.");
                }
            }
        }
        Kind::History => bounded(
            w,
            p,
            &serde_json::to_string_pretty(&value).unwrap_or_default(),
        ),
        Kind::Tests => {
            if let Some(tests) = value.as_array() {
                let mut seen = HashSet::new();
                for test in tests.iter().take(200) {
                    let (Some(id), Some(task), Some(path)) = (
                        test["id"].as_str(),
                        test["task_id"].as_str(),
                        test["path"].as_str(),
                    ) else {
                        continue;
                    };
                    if !safe_path(path) {
                        continue;
                    }
                    let file = test["target_kind"].as_str() == Some("file");
                    if file && !seen.insert((task.to_owned(), path.to_owned())) {
                        continue;
                    }
                    let supported = file
                        || (test["target_kind"].as_str() == Some("named")
                            && matches!(
                                test["provider"].as_str(),
                                Some("cargo" | "python" | "go")
                            ));
                    open_control(
                        w,
                        p,
                        c,
                        &work,
                        path,
                        test["line"].as_u64().unwrap_or(1).saturating_sub(1) as usize,
                        0,
                        &format!(
                            "{} · {}:{}",
                            test["label"].as_str().unwrap_or(id),
                            path,
                            test["line"].as_u64().unwrap_or(1)
                        ),
                    );
                    control(
                        w,
                        p,
                        c,
                        if file {
                            "Run test file"
                        } else {
                            "Run named test"
                        },
                        Action::RunTest {
                            task: task.into(),
                            test: id.into(),
                        },
                        supported && !c.workbench.run_uncertain.contains(&work),
                    );
                    if !supported {
                        caption(
                            w,
                            p,
                            "Target not addressable individually · use its detected project task.",
                        );
                    }
                }
                if tests.is_empty() {
                    caption(w, p, "No discovered project tests.");
                }
            }
            control(w, p, c, "Refresh tests", Action::Tests, true);
            control(w, p, c, "Run history", Action::Runs, true);
        }
        Kind::Tasks => {
            if let Some(tasks) = value.as_array() {
                for task in tasks.iter().take(100) {
                    if let Some(id) = task["id"].as_str() {
                        let enabled = task["available"].as_bool() != Some(false)
                            && !c.workbench.run_uncertain.contains(&work);
                        control(
                            w,
                            p,
                            c,
                            &format!("Run {}", task["label"].as_str().unwrap_or(id)),
                            Action::Run(id.into()),
                            enabled,
                        );
                        if let Some(args) = task["argv"].as_array() {
                            caption(
                                w,
                                p,
                                &args
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            );
                        }
                        if !enabled {
                            caption(
                                w,
                                p,
                                "Unavailable or previous start not reconciled. Inspect run history / package requirements.",
                            );
                        }
                    }
                }
                if tasks.is_empty() {
                    caption(w, p, "No detected project tasks.");
                }
            }
            control(w, p, c, "Run history", Action::Runs, true);
        }
        Kind::Runs => {
            if let Some(runs) = value["runs"].as_array() {
                // A list can lag a timed-out admission; it is not proof of absence.
                if c.workbench.run_uncertain.contains(&work) {
                    caption(w, p, "Previous start has unknown outcome. Do not replay it; inspect retained runs and reconcile the original admission.");
                }
                for run in runs.iter().take(64) {
                    if run["work_id"].as_str() == Some(&work) {
                        if let Some(id) = run["run_id"].as_str() {
                            control(
                                w,
                                p,
                                c,
                                &format!("{} · {}", id, run["state"].as_str().unwrap_or("unknown")),
                                Action::Output(id.into()),
                                true,
                            );
                        }
                    }
                }
                if runs.is_empty() {
                    caption(
                        w,
                        p,
                        "No retained runs. A timed-out start may still require another refresh before retrying.",
                    );
                }
                if value["truncated"].as_bool() == Some(true) {
                    caption(w, p, "Run history is bounded.");
                }
            }
            control(w, p, c, "Refresh history", Action::Runs, true);
        }
        Kind::Run { .. } | Kind::Output(_) | Kind::Cancel(_) => {
            if value["work_id"].as_str() != Some(&work) {
                caption(w, p, "Run identity mismatch; inspect run history.");
                return;
            }
            let Some(id) = value["run_id"].as_str() else {
                caption(w, p, "Run identity missing; inspect run history.");
                return;
            };
            if matches!(command.kind, Kind::Run { .. }) {
                c.workbench.run_uncertain.remove(&work);
            }
            let state = value["state"].as_str().unwrap_or("unknown");
            caption(w, p, &format!("{id} · {state}"));
            bounded(
                w,
                p,
                value["stdout"]
                    .as_str()
                    .or_else(|| value["result"]["stdout"].as_str())
                    .unwrap_or(""),
            );
            bounded(
                w,
                p,
                value["stderr"]
                    .as_str()
                    .or_else(|| value["result"]["stderr"].as_str())
                    .unwrap_or(""),
            );
            if value["output_truncated"].as_bool() == Some(true) {
                caption(w, p, "Daemon output truncated.");
            }
            control(w, p, c, "Refresh output", Action::Output(id.into()), true);
            if matches!(state, "running" | "ready") {
                control(w, p, c, "Cancel run", Action::Cancel(id.into()), true);
            }
            if let Some(locations) = value["locations"].as_array() {
                for loc in locations.iter().take(100) {
                    if let Some(path) = loc["path"].as_str() {
                        open_control(
                            w,
                            p,
                            c,
                            &work,
                            path,
                            loc["line"].as_u64().unwrap_or(1).saturating_sub(1) as usize,
                            loc["column"].as_u64().unwrap_or(1).saturating_sub(1) as usize,
                            loc["message"].as_str().unwrap_or(path),
                        );
                    }
                }
            }
        }
        Kind::Language { path, feature, .. } => {
            let root = value["root"].as_str().unwrap_or("");
            let obs = &value["observation"];
            if obs["ok"].as_bool() == Some(false) {
                bounded(w, p, &obs.to_string());
                return;
            }
            let result = result(obs);
            match feature {
                Feature::Hover => bounded(
                    w,
                    p,
                    &serde_json::to_string_pretty(result).unwrap_or_default(),
                ),
                _ => {
                    language_rows(w, p, c, &work, &path, root, result, 0);
                    if result.is_null() || result.as_array().is_some_and(Vec::is_empty) {
                        caption(w, p, "No language results for the saved file.");
                    }
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "code_workbench_tests.rs"]
mod tests;
