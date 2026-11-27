//! Retained Forge review reader. Sealed evidence is never substituted with the
//! working checkout. Every mutation is explicit, revision-bound and not replayed.
use medousa_sdk::{MedousaClient, operations as ops};
use rndr_core::*;
use rndr_ui::controls::HostIntent;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
#[cfg(test)]
#[path = "reviews_tests.rs"]
pub(crate) mod tests;
#[path = "reviews_view.rs"]
mod view;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Revision {
    work: String,
    attempt: String,
    evidence: String,
    digest: String,
    baseline: String,
    head: String,
}
fn string(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_owned()
}
fn revision(v: &Value) -> Option<Revision> {
    let r = Revision {
        work: string(v, "work_id"),
        attempt: string(v, "attempt_id"),
        evidence: string(v, "evidence_id"),
        digest: string(v, "evidence_digest"),
        baseline: string(v, "baseline_oid"),
        head: string(v, "sealed_head_oid"),
    };
    [
        &r.work,
        &r.attempt,
        &r.evidence,
        &r.digest,
        &r.baseline,
        &r.head,
    ]
    .iter()
    .all(|s| !s.is_empty())
    .then_some(r)
}
fn allowed(v: &Value, action: &str) -> bool {
    v["allowed_actions"][action]["allowed"] == true
}
fn approvable(v: &Value) -> bool {
    revision(v).is_some()
        && allowed(v, "review")
        && v["changed_files"].as_array().is_some_and(|a| !a.is_empty())
        && v["truncated"] != true
        && v["base_advanced"] != true
        && v["synthesis"]["blocks_approval"] != true
        && v["synthesis"]["verification"]["success"] != false
        && !v["synthesis"]["issues"]
            .as_array()
            .is_some_and(|a| a.iter().any(|i| i["blocks_approval"] == true))
        && !v["policy"]["violations"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
}
#[derive(Clone, Debug)]
enum Kind {
    Queue,
    Review {
        work: String,
        attempt: Option<String>,
    },
    File {
        revision: Revision,
        path: String,
    },
    Mutate {
        revision: Revision,
        mutation: Mutation,
    },
}
#[derive(Clone, Debug)]
enum Mutation {
    Approve {
        strategy: String,
        rationale: String,
    },
    Request(String),
    Apply(String),
    Comment {
        path: String,
        side: String,
        line: u64,
        body: String,
        anchor: String,
    },
    Resolve(String),
    /// Put one file back to its baseline in the reviewed branch.
    Restore {
        path: String,
        oid: String,
    },
}
#[derive(Clone, Debug)]
pub struct Command {
    serial: u64,
    kind: Kind,
}
pub struct Update {
    command: Command,
    result: Result<Value, String>,
}
#[derive(Clone)]
enum Action {
    Refresh,
    Open(String),
    Candidate(String),
    File(String),
    ToggleFile(String),
    Context(String),
    ExpandAll,
    CollapseAll,
    Summary,
    Changes,
    About,
    OpenInCode(String),
    Restore(String),
    Symbols(String),
    Filter(u8),
    Approve,
    Request,
    Apply,
    Comment,
    Resolve(String),
    Side,
    Strategy,
    Confirm,
    Cancel,
}
#[derive(Clone, Default)]
struct Draft {
    body: String,
    line: String,
    old: bool,
}
#[derive(Default)]
struct State {
    queue_panel: Option<Entity>,
    queue_tools: Option<Entity>,
    detail: Option<Entity>,
    form: Option<Entity>,
    search: Option<Entity>,
    body: Option<Entity>,
    line: Option<Entity>,
    items: Vec<Value>,
    query: String,
    work: Option<String>,
    review: Value,
    file: Option<(String, Value)>,
    expanded: HashSet<String>,
    /// Files whose changed-symbol list is open.
    symbols: HashSet<String>,
    /// (work, path) to show in Code once this review's state is stored.
    open_code: Option<(String, String)>,
    context: HashSet<String>,
    context_cache: HashMap<String, Option<Value>>,
    file_cache: HashMap<String, Value>,
    file_errors: HashSet<String>,
    loading_path: Option<String>,
    diff_positions: HashMap<String, (rndr_ui::collections::VariableAnchor, f32)>,
    drafts: HashMap<(String, String), Draft>,
    draft: Draft,
    actions: HashMap<u64, Action>,
    commands: Vec<Command>,
    serial: u64,
    queue_pending: Option<u64>,
    read_pending: Option<u64>,
    file_pending: Option<u64>,
    mutation_pending: Option<u64>,
    confirmation: Option<Mutation>,
    uncertain: bool,
    loaded: bool,
    redraw: bool,
    message: String,
    viewport: (bool, u32),
    changes: bool,
    about: bool,
    filter: u8,
    strategy: bool, // false: preserve branch; true: fast-forward only
}
fn enqueue(s: &mut State, kind: Kind) -> u64 {
    s.serial += 1;
    let serial = s.serial;
    s.commands.push(Command { serial, kind });
    serial
}
fn refresh(s: &mut State) {
    if s.mutation_pending.is_some() {
        return;
    }
    s.queue_pending = Some(enqueue(s, Kind::Queue));
    if let Some(work) = s.work.clone() {
        // Pin the revision already being read, even if a new candidate arrives.
        let attempt = s.review["attempt_id"].as_str().map(str::to_owned);
        s.read_pending = Some(enqueue(s, Kind::Review { work, attempt }));
    }
    s.confirmation = None;
    s.redraw = true;
}
fn capture(w: &World, s: &mut State) {
    if let Some(v) = s.body.and_then(|e| w.get::<Label>(e)) {
        s.draft.body = v.text.clone();
    }
    if let Some(v) = s.line.and_then(|e| w.get::<Label>(e)) {
        s.draft.line = v.text.clone();
    }
}
fn draft_key(s: &State) -> (String, String) {
    (
        string(&s.review, "evidence_id"),
        s.file.as_ref().map(|f| f.0.clone()).unwrap_or_default(),
    )
}
fn save_draft(s: &mut State) {
    s.drafts.insert(draft_key(s), s.draft.clone());
}
fn load_draft(s: &mut State) {
    s.draft = s.drafts.get(&draft_key(s)).cloned().unwrap_or_default();
}
// Cache and expansion state never outlive their exact sealed revision. Reads are
// serialized so Expand all cannot flood the daemon with unbounded concurrency.
fn clear_files(s: &mut State) {
    s.expanded.clear();
    s.context.clear();
    s.context_cache.clear();
    s.file_cache.clear();
    s.file_errors.clear();
    s.loading_path = None;
    s.diff_positions.clear();
    s.file_pending = None;
}
fn select_file(s: &mut State, path: &str) {
    save_draft(s);
    s.file = Some((
        path.into(),
        s.file_cache.get(path).cloned().unwrap_or(Value::Null),
    ));
    load_draft(s);
}
fn load_next_file(s: &mut State) {
    if s.file_pending.is_some() || s.read_pending.is_some() || s.mutation_pending.is_some() {
        return;
    }
    let Some(revision) = revision(&s.review) else {
        return;
    };
    let path = s.review["changed_files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["path"].as_str())
        .find(|path| {
            s.expanded.contains(*path)
                && !s.file_cache.contains_key(*path)
                && !s.file_errors.contains(*path)
        })
        .map(str::to_owned);
    if let Some(path) = path {
        s.loading_path = Some(path.clone());
        s.file_pending = Some(enqueue(s, Kind::File { revision, path }));
    }
}
fn known_file(s: &State, path: &str) -> bool {
    s.review["changed_files"]
        .as_array()
        .is_some_and(|files| files.iter().any(|f| f["path"].as_str() == Some(path)))
}

pub(crate) fn can_rebind(w: &mut World) -> bool {
    let Some(mut s) = w.resource_mut::<State>().map(std::mem::take) else {
        return true;
    };
    capture(w, &mut s);
    let safe = s.mutation_pending.is_none()
        && !s.uncertain
        && s.confirmation.is_none()
        && s.draft.body.trim().is_empty()
        && s.drafts.values().all(|d| d.body.trim().is_empty());
    w.insert_resource(s);
    safe
}
/// Current review's retained detail + decision form (not its queue sidebar).
pub(crate) fn workspace_item(w: &World) -> Option<(String, Entity)> {
    let s = w.resource::<State>()?;
    let detail = s.detail?;
    Some((s.work.clone()?, w.get::<Parent>(detail)?.entity))
}
/// Reviews owns one retained detail form; refuse switching while a decision is pending.
pub(crate) fn workspace_open(w: &mut World, work_id: &str) -> bool {
    if work_id.is_empty() || w.resource::<State>().is_none() {
        return false;
    }
    if w.resource::<State>()
        .is_some_and(|s| s.work.as_deref() == Some(work_id))
    {
        return true;
    }
    if !can_rebind(w) {
        return false;
    }
    let mut s = std::mem::take(w.resource_mut::<State>().unwrap());
    capture(w, &mut s);
    open_review(&mut s, work_id.to_owned());
    w.insert_resource(s);
    true
}
fn open_review(s: &mut State, work: String) {
    save_draft(s);
    s.changes = false;
    s.about = false;
    s.work = Some(work.clone());
    s.review = Value::Null;
    s.file = None;
    clear_files(s);
    s.confirmation = None;
    s.uncertain = false;
    s.message.clear();
    load_draft(s);
    s.read_pending = Some(enqueue(
        s,
        Kind::Review {
            work,
            attempt: None,
        },
    ));
    s.redraw = true;
}
pub fn mount(w: &mut World, parent: Entity) {
    view::mount(w, parent);
}
pub fn activate(w: &mut World) {
    let Some(mut s) = w.resource_mut::<State>().map(std::mem::take) else {
        return;
    };
    if !s.loaded {
        s.loaded = true;
        refresh(&mut s);
    }
    w.insert_resource(s);
}
pub fn sync(w: &mut World) {
    let Some(mut s) = w.resource_mut::<State>().map(std::mem::take) else {
        return;
    };
    capture(w, &mut s);
    let viewport = w
        .resource::<Viewport>()
        .map_or((true, 850), |v| (v.width >= 1100., v.height as u32));
    if viewport != s.viewport {
        s.viewport = viewport;
        s.redraw = true;
    }
    let query = s
        .search
        .and_then(|e| w.get::<Label>(e))
        .map(|v| v.text.clone())
        .unwrap_or_default();
    if query != s.query {
        s.query = query;
        s.redraw = true;
    }
    if s.redraw {
        view::render(w, &mut s);
        s.redraw = false;
    }
    w.insert_resource(s);
}
pub fn action(w: &mut World, host: &HostIntent) -> bool {
    let HostIntent::Command(id) = host else {
        return false;
    };
    let Some(a) = w
        .resource::<State>()
        .and_then(|s| s.actions.get(id))
        .cloned()
    else {
        return false;
    };
    let mut s = std::mem::take(w.resource_mut::<State>().unwrap());
    capture(w, &mut s);
    if s.mutation_pending.is_none() {
        match a {
            Action::Refresh => refresh(&mut s),
            Action::Open(work) => {
                open_review(&mut s, work);
            }
            Action::Candidate(attempt) => {
                if let Some(work) = s.work.clone() {
                    save_draft(&mut s); s.review = Value::Null; s.file = None; clear_files(&mut s); s.file_pending = None; s.confirmation = None; load_draft(&mut s);
                    s.read_pending = Some(enqueue(&mut s,Kind::Review { work,attempt:Some(attempt) }));
                }
            }
            Action::File(path) if known_file(&s, &path) && s.read_pending.is_none() => {
                s.changes = true;
                s.expanded.insert(path.clone());
                s.file_errors.remove(&path);
                select_file(&mut s, &path);
                load_next_file(&mut s);
            }
            Action::ToggleFile(path) if known_file(&s, &path) && s.read_pending.is_none() => {
                if !s.expanded.remove(&path) {
                    s.expanded.insert(path.clone());
                    s.file_errors.remove(&path);
                    select_file(&mut s, &path);
                    load_next_file(&mut s);
                }
            }
            Action::ExpandAll if s.read_pending.is_none() => {
                s.expanded = s.review["changed_files"].as_array().into_iter().flatten()
                    .filter_map(|f| f["path"].as_str().map(str::to_owned)).collect();
                s.file_errors.clear();
                load_next_file(&mut s);
            }
            Action::Context(path) if known_file(&s, &path) => {
                if !s.context.remove(&path) { s.context.insert(path.clone()); }
                s.diff_positions.remove(&path);
            }
            Action::CollapseAll => s.expanded.clear(),
            Action::Changes => s.changes = true,
            Action::About => s.about = !s.about,
            Action::OpenInCode(path) if known_file(&s, &path) => {
                if let Some(work) = s.work.clone() { s.open_code = Some((work, path)); }
            }
            Action::Restore(path) if ready(&s) && known_file(&s, &path) => {
                if let Some(oid) = s.file_cache.get(&path).and_then(|v| v["reviewed_oid"].as_str()).filter(|o| !o.is_empty()) {
                    s.confirmation = Some(Mutation::Restore { path: path.clone(), oid: oid.to_owned() });
                }
            }
            Action::Symbols(path) => { if !s.symbols.remove(&path) { s.symbols.insert(path); } }
            Action::Filter(filter) => s.filter = filter,
            Action::Summary => { s.changes = false; save_draft(&mut s); s.file = None; load_draft(&mut s); }
            Action::Side => s.draft.old = !s.draft.old,
            Action::Strategy => s.strategy = !s.strategy,
            Action::Cancel => s.confirmation = None,
            Action::Approve if ready(&s) && approvable(&s.review) => s.confirmation = Some(Mutation::Approve { strategy: if s.strategy {"fast_forward_only"} else {"preserve_branch"}.into(), rationale:s.draft.body.trim().into() }),
            Action::Request if ready(&s) && allowed(&s.review,"review") && !s.draft.body.trim().is_empty() => s.confirmation = Some(Mutation::Request(s.draft.body.trim().into())),
            Action::Apply if ready(&s) && allowed(&s.review,"apply") => {
                let id = string(&s.review["decision"],"id");
                if !id.is_empty() { s.confirmation = Some(Mutation::Apply(id)); }
            }
            Action::Comment if ready(&s) && comment_valid(&s) => {
                let path = s.file.as_ref().unwrap().0.clone(); let line = s.draft.line.trim().parse().unwrap();
                s.confirmation = Some(Mutation::Comment {path,side:if s.draft.old {"old"} else {"new"}.into(),line,body:s.draft.body.trim().into(),anchor:comment_anchor(&s)});
            }
            Action::Resolve(id) if ready(&s) => s.confirmation = Some(Mutation::Resolve(id)),
            Action::Confirm if ready(&s) => {
                if let (Some(revision),Some(mutation)) = (revision(&s.review),s.confirmation.take()) {
                    // Recheck local affordances; the daemon revalidates exact evidence before committing.
                    if mutation_allowed(&s,&mutation) {
                        s.mutation_pending = Some(enqueue(&mut s,Kind::Mutate {revision,mutation}));
                    }
                }
            }
            Action::Comment => s.message = "Enter a nonempty comment and a valid line number shown in the selected old/new diff side.".into(),
            Action::Request => s.message = "Enter a change-request rationale first.".into(),
            _ => {}
        }
    }
    view::render(w, &mut s);
    s.redraw = false;
    let open = s.open_code.take();
    w.insert_resource(s);
    if let Some((work, path)) = open {
        crate::code::open_file(w, &work, &path);
    }
    true
}
fn ready(s: &State) -> bool {
    !s.uncertain
        && s.read_pending.is_none()
        && s.mutation_pending.is_none()
        && revision(&s.review).is_some()
}
fn mutation_allowed(s: &State, m: &Mutation) -> bool {
    match m {
        Mutation::Approve { .. } => approvable(&s.review),
        Mutation::Request(body) => allowed(&s.review, "review") && !body.is_empty(),
        Mutation::Apply(id) => {
            allowed(&s.review, "apply") && s.review["decision"]["id"].as_str() == Some(id)
        }
        Mutation::Comment { .. } => comment_valid(s),
        // As medousa-home: the daemon checks the reviewed blob before writing.
        Mutation::Restore { path, .. } => known_file(s, path),
        Mutation::Resolve(id) => {
            allowed(&s.review, "review")
                && s.review["comments"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|c| c["id"].as_str() == Some(id) && c["resolved_at"].is_null())
                })
        }
    }
}
fn comment_valid(s: &State) -> bool {
    if !allowed(&s.review, "review") || s.draft.body.trim().is_empty() {
        return false;
    }
    let Some((_, v)) = &s.file else {
        return false;
    };
    let Ok(line) = s.draft.line.trim().parse::<u64>() else {
        return false;
    };
    let key = if s.draft.old { "old_line" } else { "new_line" };
    v["binary"] != true
        && line > 0
        && v["hunks"].as_array().is_some_and(|h| {
            h.iter().any(|h| {
                h["lines"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|l| l[key].as_u64() == Some(line)))
            })
        })
}
fn comment_anchor(s: &State) -> String {
    let key = if s.draft.old { "old_line" } else { "new_line" };
    let line = s.draft.line.trim().parse::<u64>().ok();
    s.file
        .as_ref()
        .and_then(|(_, v)| v["hunks"].as_array())
        .into_iter()
        .flatten()
        .flat_map(|h| h["lines"].as_array().into_iter().flatten())
        .find(|l| l[key].as_u64().is_some() && l[key].as_u64() == line)
        .and_then(|l| l["content"].as_str())
        .unwrap_or_default()
        .to_owned()
}
pub fn take_commands(w: &mut World) -> Vec<Command> {
    w.resource_mut::<State>()
        .map(|s| std::mem::take(&mut s.commands))
        .unwrap_or_default()
}
fn route(op: ops::Operation, work: &str) -> String {
    op.path.replace("{work_id}", &urlencoding::encode(work))
}
async fn queue(client: &MedousaClient) -> Result<Value, medousa_sdk::SdkError> {
    let mut items = Vec::new();
    let mut cursor = None;
    let mut seen = HashSet::new();
    loop {
        let mut query = vec![("limit", "256".to_owned())];
        if let Some(cursor) = cursor.take() {
            query.push(("cursor", cursor));
        }
        let v: Value = client
            .http()
            .get_query(ops::FORGE_ITEMS_GET.path, &query)
            .await?;
        if let Some(a) = v.as_array() {
            items.extend(a.iter().cloned());
            break;
        }
        items.extend(
            v["items"]
                .as_array()
                .ok_or_else(|| medousa_sdk::SdkError::Serde("Missing review queue".into()))?
                .iter()
                .cloned(),
        );
        if v["truncated"] != true {
            break;
        }
        let next = v["next_cursor"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| medousa_sdk::SdkError::Serde("Incomplete review queue".into()))?;
        if !seen.insert(next.to_owned()) || seen.len() > 64 {
            return Err(medousa_sdk::SdkError::Serde(
                "Review pagination exceeded bounds".into(),
            ));
        }
        cursor = Some(next.to_owned());
    }
    Ok(Value::Array(items))
}
fn mutation_body(r: &Revision, m: &Mutation) -> (ops::Operation, Value) {
    match m {
        Mutation::Approve {
            strategy,
            rationale,
        } => (
            ops::FORGE_ITEMS_BY_WORK_ID_DECISIONS_POST,
            json!({"evidence_id":r.evidence,"evidence_digest":r.digest,"strategy":strategy,"rationale":rationale,"acknowledged_violations":[]}),
        ),
        Mutation::Request(summary) => (
            ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_REQUEST_CHANGES_POST,
            json!({"evidence_id":r.evidence,"evidence_digest":r.digest,"summary":summary}),
        ),
        Mutation::Apply(id) => (
            ops::FORGE_ITEMS_BY_WORK_ID_APPLY_POST,
            json!({"decision_id":id}),
        ),
        Mutation::Comment {
            path,
            side,
            line,
            body,
            anchor,
        } => (
            ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_COMMENTS_POST,
            json!({"evidence_id":r.evidence,"attempt_id":r.attempt,"path":path,"side":side,"start_line":line,"end_line":line,"body":body,"anchor_text":anchor}),
        ),
        Mutation::Resolve(_) => (
            ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_COMMENTS_BY_COMMENT_ID_PATCH,
            json!({"resolve":true}),
        ),
        Mutation::Restore { path, oid } => (
            ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_FILE_POST,
            json!({"path":path,"expected_reviewed_oid":oid,"attempt_id":r.attempt}),
        ),
    }
}
pub async fn execute(client: &MedousaClient, command: Command) -> Update {
    let result: Result<Value, medousa_sdk::SdkError> = async {
        match &command.kind {
            Kind::Queue => queue(client).await,
            Kind::Review { work, attempt } => {
                let query: Vec<_> = attempt.iter().map(|s| ("attempt_id", s.clone())).collect();
                client
                    .http()
                    .get_query(&route(ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_GET, work), &query)
                    .await
            }
            Kind::File { revision: r, path } => {
                client
                    .http()
                    .get_query(
                        &route(ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_FILE_GET, &r.work),
                        &[("attempt_id", r.attempt.clone()), ("path", path.clone())],
                    )
                    .await
            }
            Kind::Mutate {
                revision: r,
                mutation,
            } => {
                // Check fresh affordances and full sealed identity immediately before any write.
                let fresh: Value = client
                    .http()
                    .get_query(
                        &route(ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_GET, &r.work),
                        &[("attempt_id", r.attempt.clone())],
                    )
                    .await?;
                let state = State {
                    review: fresh.clone(),
                    ..Default::default()
                };
                let permitted = match mutation {
                    Mutation::Comment { .. } => allowed(&fresh, "review"),
                    _ => mutation_allowed(&state, mutation),
                };
                if revision(&fresh).as_ref() != Some(r) || !permitted {
                    return Err(medousa_sdk::SdkError::Serde(
                        "Review changed or action is no longer allowed. Refresh before deciding."
                            .into(),
                    ));
                }
                let (op, body) = mutation_body(r, mutation);
                let mut path = route(op, &r.work);
                if let Mutation::Resolve(id) = mutation {
                    path = path.replace("{comment_id}", &urlencoding::encode(id));
                    client.http().patch(&path, &body).await
                } else {
                    client.http().post(&path, &body).await
                }
            }
        }
    }
    .await;
    Update { command, result:result.map_err(|_| "Review request failed or its outcome is unconfirmed. Refresh to reconcile; no mutation was retried.".into()) }
}
pub fn apply(w: &mut World, u: Update) {
    let Some(mut s) = w.resource_mut::<State>().map(std::mem::take) else {
        return;
    };
    capture(w, &mut s);
    let serial = u.command.serial;
    let expected = match &u.command.kind {
        Kind::Queue => s.queue_pending,
        Kind::Review { .. } => s.read_pending,
        Kind::File { .. } => s.file_pending,
        Kind::Mutate { .. } => s.mutation_pending,
    };
    if expected != Some(serial) {
        w.insert_resource(s);
        return;
    }
    match &u.command.kind {
        Kind::Queue => s.queue_pending = None,
        Kind::Review { .. } => s.read_pending = None,
        Kind::File { .. } => s.file_pending = None,
        Kind::Mutate { .. } => s.mutation_pending = None,
    }
    match (u.command.kind, u.result) {
        (Kind::Queue, Ok(v)) => {
            s.items = v.as_array().cloned().unwrap_or_default();
        }
        (Kind::Review { work, attempt }, Ok(v)) => {
            if s.work.as_ref() == Some(&work)
                && v["work_id"].as_str() == Some(&work)
                && attempt
                    .as_ref()
                    .is_none_or(|a| v["attempt_id"].as_str() == Some(a))
            {
                save_draft(&mut s);
                let changed = revision(&s.review) != revision(&v);
                s.review = v;
                s.uncertain = false;
                s.confirmation = None;
                if changed {
                    s.file = None;
                    clear_files(&mut s);
                }
                load_draft(&mut s);
                s.message.clear();
            } else {
                s.uncertain = true;
                s.message = "Review identity mismatch. Refresh before deciding.".into();
            }
        }
        (Kind::File { revision: r, path }, Ok(v)) => {
            if revision(&s.review).as_ref() == Some(&r)
                && s.loading_path.as_deref() == Some(&path)
                && file_matches(&v, &r, &path)
            {
                s.file_cache.insert(path.clone(), v.clone());
                if s.file.as_ref().is_some_and(|f| f.0 == path) {
                    s.file = Some((path, v));
                }
            } else {
                s.file_errors.insert(path);
                s.message = "Sealed diff identity mismatch. No file was substituted.".into();
            }
        }
        (Kind::Mutate { mutation, .. }, Ok(v)) => {
            s.confirmation = None;
            s.message = "Action accepted. Refreshing review…".into();
            if let (Mutation::Restore { path, .. }, Some(work)) = (&mutation, s.work.clone()) {
                let path = v["path"].as_str().unwrap_or(path).to_owned();
                s.message = format!("Restored {path} to before this change. Refreshing review…");
                s.open_code = Some((work, path));
            }
            if matches!(mutation, Mutation::Comment { .. } | Mutation::Request(_)) {
                s.draft.body.clear();
                save_draft(&mut s);
            }
            s.uncertain = true;
            refresh(&mut s);
        }
        (kind, Err(error)) => {
            if let Kind::File { path, .. } = &kind {
                s.file_errors.insert(path.clone());
            }
            s.message = error;
            if matches!(kind, Kind::Mutate { .. } | Kind::Review { .. }) {
                s.uncertain = true;
            }
            s.confirmation = None;
        }
    }
    if s.file_pending.is_none() {
        s.loading_path = None;
    }
    load_next_file(&mut s);
    view::render(w, &mut s);
    s.redraw = false;
    let open = s.open_code.take();
    w.insert_resource(s);
    if let Some((work, path)) = open {
        crate::code::open_file(w, &work, &path);
    }
}
fn file_matches(v: &Value, r: &Revision, path: &str) -> bool {
    v["work_id"].as_str() == Some(&r.work)
        && v["attempt_id"].as_str() == Some(&r.attempt)
        && v["path"].as_str() == Some(path)
        && v["baseline_oid"].as_str() == Some(&r.baseline)
        && v["reviewed_oid"].as_str() == Some(&r.head)
}

#[cfg(test)]
mod workspace_open_tests {
    use super::*;
    #[test]
    fn workspace_open_exact_review_without_navigation_and_preserves_draft() {
        let mut app = crate::ui::app();
        app.update();
        let w = app.world_mut();
        let before = crate::surfaces::active(w);
        assert!(workspace_open(w, "exact-work"));
        assert_eq!(crate::surfaces::active(w), before);
        assert!(w.resource::<State>().unwrap().commands.iter().any(
            |c| matches!(&c.kind, Kind::Review { work, attempt: None } if work == "exact-work")
        ));
        w.resource_mut::<State>().unwrap().draft.body = "unsubmitted decision".into();
        // No mounted body exists until render, so capture leaves the draft intact.
        assert!(!workspace_open(w, "different-work"));
        assert_eq!(
            w.resource::<State>().unwrap().work.as_deref(),
            Some("exact-work")
        );
        assert!(workspace_open(w, "exact-work"));
    }
}
