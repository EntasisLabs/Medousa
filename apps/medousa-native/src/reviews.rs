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
    Summary,
    Changes,
    About,
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
                save_draft(&mut s); s.changes = false; s.about = false; s.work = Some(work.clone()); s.review = Value::Null; s.file = None;
                s.file_pending = None; s.confirmation = None; s.uncertain = false; s.message.clear(); load_draft(&mut s);
                s.read_pending = Some(enqueue(&mut s,Kind::Review { work,attempt:None }));
            }
            Action::Candidate(attempt) => {
                if let Some(work) = s.work.clone() {
                    save_draft(&mut s); s.review = Value::Null; s.file = None; s.file_pending = None; s.confirmation = None; load_draft(&mut s);
                    s.read_pending = Some(enqueue(&mut s,Kind::Review { work,attempt:Some(attempt) }));
                }
            }
            Action::File(path) => {
                if let Some(revision) = revision(&s.review).filter(|_| s.read_pending.is_none()) {
                    save_draft(&mut s); s.changes = true; s.file = Some((path.clone(),Value::Null)); load_draft(&mut s);
                    s.file_pending = Some(enqueue(&mut s,Kind::File { revision,path }));
                }
            }
            Action::Changes => s.changes = true,
            Action::About => s.about = !s.about,
            Action::Filter(filter) => s.filter = filter,
            Action::Summary => { s.changes = false; save_draft(&mut s); s.file = None; s.file_pending = None; load_draft(&mut s); }
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
    w.insert_resource(s);
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
    if !allowed(&s.review, "review") || s.draft.body.trim().is_empty() || s.file_pending.is_some() {
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
                    s.file_pending = None;
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
                && s.file.as_ref().is_some_and(|f| f.0 == path)
                && file_matches(&v, &r, &path)
            {
                s.file = Some((path, v));
            } else {
                s.message = "Sealed diff identity mismatch. No file was substituted.".into();
            }
        }
        (Kind::Mutate { mutation, .. }, Ok(_)) => {
            s.confirmation = None;
            s.message = "Action accepted. Refreshing review…".into();
            if matches!(mutation, Mutation::Comment { .. } | Mutation::Request(_)) {
                s.draft.body.clear();
                save_draft(&mut s);
            }
            s.uncertain = true;
            refresh(&mut s);
        }
        (kind, Err(error)) => {
            s.message = error;
            if matches!(kind, Kind::Mutate { .. } | Kind::Review { .. }) {
                s.uncertain = true;
            }
            s.confirmation = None;
        }
    }
    view::render(w, &mut s);
    s.redraw = false;
    w.insert_resource(s);
}
fn file_matches(v: &Value, r: &Revision, path: &str) -> bool {
    v["work_id"].as_str() == Some(&r.work)
        && v["attempt_id"].as_str() == Some(&r.attempt)
        && v["path"].as_str() == Some(path)
        && v["baseline_oid"].as_str() == Some(&r.baseline)
        && v["reviewed_oid"].as_str() == Some(&r.head)
}
