//! Ask Medousa, after medousa-home's: describe a change to the selection and
//! Medousa answers with a proposal in the project's own conversation. A
//! proposal shows over the canvas as a preview until you use or dismiss it;
//! proposals Medousa makes from a chat land here the same way. Requests keep
//! their id, so a lost reply is retried without doubling the work.
use super::*;
use medousa_types::{
    CreativeJob, CreativeJobRequest, CreativeJobStatus, CreativeProposal, CreativeProposalDecision,
    CreativeProposalStatus,
};

/// How often a running request is checked.
const JOB_POLL: std::time::Duration = std::time::Duration::from_secs(2);
/// medousa-home's request limits.
const MAX_PROMPT: usize = 16 * 1024;
const MAX_SELECTED: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// Asked here.
    Job,
    /// Proposed by Medousa from a conversation.
    Proposal,
}

/// The latest request or proposal, as this surface shows it.
#[derive(Clone, Debug)]
pub(crate) struct View {
    pub id: String,
    pub source: Source,
    pub status: CreativeJobStatus,
    pub session: String,
    pub label: Option<String>,
    pub operations: Vec<rndr_canvas::Operation>,
    /// The revision the proposal was made against.
    pub base: Option<u64>,
    pub error: Option<String>,
}

impl View {
    pub(crate) fn from_job(job: CreativeJob) -> Self {
        let (label, operations, base) = match job.result {
            Some(r) => (
                Some(r.label),
                convert(&r.operations).unwrap_or_default(),
                Some(r.expected_revision),
            ),
            None => (None, vec![], None),
        };
        Self {
            id: job.request.id,
            source: Source::Job,
            status: job.status,
            session: job.session_id,
            label,
            operations,
            base,
            error: job.error,
        }
    }
    pub(crate) fn from_proposal(p: CreativeProposal) -> Self {
        Self {
            id: p.request.id,
            source: Source::Proposal,
            status: match p.status {
                CreativeProposalStatus::Ready => CreativeJobStatus::Ready,
                CreativeProposalStatus::Accepted => CreativeJobStatus::Accepted,
                CreativeProposalStatus::Cancelled => CreativeJobStatus::Cancelled,
            },
            session: String::new(),
            label: Some(p.request.label),
            operations: convert(&p.request.operations).unwrap_or_default(),
            base: Some(p.request.expected_revision),
            error: None,
        }
    }
    /// Queued, dispatching or running.
    pub(crate) fn active(&self) -> bool {
        matches!(
            self.status,
            CreativeJobStatus::Queued | CreativeJobStatus::Dispatching | CreativeJobStatus::Running
        )
    }
    /// A proposal waiting for a decision.
    pub(crate) fn ready(&self) -> bool {
        matches!(
            self.status,
            CreativeJobStatus::Ready | CreativeJobStatus::Stale
        ) && !self.operations.is_empty()
    }
}

#[derive(Default)]
pub(crate) struct Ask {
    pub open: bool,
    pub field: Option<Entity>,
    pub draft: String,
    pub view: Option<View>,
    /// A request sent without a confirmed reply, kept for an exact retry.
    pub pending: Option<CreativeJobRequest>,
    /// A decision sent without a confirmed reply.
    pub decision: Option<CreativeProposalDecision>,
    pub preview: bool,
    pub error: Option<String>,
    /// In-flight serials: the latest-request read, a running request's
    /// check, and a send or decision.
    pub list: Option<u64>,
    pub poll: Option<u64>,
    pub busy: Option<u64>,
    pub polled: Option<std::time::Instant>,
    /// Ops last handed to the canvas as a preview, so they are set once.
    pub shown: Option<String>,
}

impl Ask {
    pub(crate) fn sending(&self) -> bool {
        self.busy.is_some()
    }
    pub(crate) fn executing(&self) -> bool {
        self.view.as_ref().is_some_and(View::active)
    }
    /// What the Ask controls show, so the chrome rebuilds when it changes.
    pub(crate) fn signature(&self) -> String {
        format!(
            "{}|{:?}|{}|{}|{}|{:?}",
            self.open,
            self.view
                .as_ref()
                .map(|v| (&v.id, v.status, &v.label, &v.error)),
            self.preview,
            self.pending.is_some(),
            self.busy.is_some(),
            self.error,
        )
    }
}

/// Every command carries the project it is for; replies for another project
/// are dropped.
#[derive(Clone, Debug)]
pub(crate) enum AskKind {
    /// The project's latest request or ready proposal.
    Latest,
    /// Check one request.
    Job(String),
    Start(CreativeJobRequest),
    Decide(Source, String, CreativeProposalDecision),
}

pub(crate) enum AskReply {
    Latest(Option<View>),
    Job(View),
    /// The decision went through; a request answers with its new state.
    Decided(Option<View>, bool),
}

pub(crate) async fn execute(
    client: &MedousaClient,
    project: &str,
    kind: &AskKind,
) -> Result<AskReply, CanvasError> {
    let api = client.creative();
    match kind {
        AskKind::Latest => {
            let jobs = api.jobs(project).await.map_err(canvas_error)?.jobs;
            let proposals = api
                .proposals(project)
                .await
                .map_err(canvas_error)?
                .proposals;
            let mut latest: Vec<(String, View)> = jobs
                .into_iter()
                .map(|j| (j.created_at.clone(), View::from_job(j)))
                .chain(
                    proposals
                        .into_iter()
                        .filter(|p| p.status == CreativeProposalStatus::Ready)
                        .map(|p| (p.created_at.clone(), View::from_proposal(p))),
                )
                .collect();
            latest.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(AskReply::Latest(latest.pop().map(|(_, v)| v)))
        }
        AskKind::Job(job) => api
            .job(project, job)
            .await
            .map(|j| AskReply::Job(View::from_job(j)))
            .map_err(canvas_error),
        AskKind::Start(request) => api
            .create_job(project, request)
            .await
            .map(|j| AskReply::Job(View::from_job(j)))
            .map_err(canvas_error),
        AskKind::Decide(Source::Job, job, decision) => api
            .decide_job(project, job, decision)
            .await
            .map(|j| AskReply::Decided(Some(View::from_job(j)), decision.accept))
            .map_err(canvas_error),
        AskKind::Decide(Source::Proposal, id, decision) => api
            .decide(project, id, decision)
            .await
            .map(|_| AskReply::Decided(None, decision.accept))
            .map_err(canvas_error),
    }
}

/// Starts a request about the selection, or says why it cannot.
pub(crate) fn start(w: &mut World, s: &mut State) {
    let draft = s.ask.draft.trim().to_string();
    let Some(v) = w.resource::<CanvasView>() else {
        return;
    };
    let e = &v.editor;
    let selected = e.selection().to_vec();
    let refused = if s.ask.sending() || s.ask.pending.is_some() || s.ask.executing() {
        Some("Finish the current request first")
    } else if draft.is_empty() || selected.is_empty() {
        Some("Select something on the canvas and describe the change")
    } else if draft.len() > MAX_PROMPT {
        Some("Shorten this request to 16 KiB or less")
    } else if selected.len() > MAX_SELECTED {
        Some("Select fewer than 1,001 objects for this request")
    } else if e.is_saving() {
        Some("Wait for your last edit to save, then ask")
    } else {
        None
    };
    if let Some(reason) = refused {
        s.ask.error = Some(reason.into());
        return;
    }
    let project = e.document_id().to_string();
    // Keep refining the same conversation; a chat proposal starts afresh.
    let parent = s
        .ask
        .view
        .as_ref()
        .filter(|v| v.source == Source::Job)
        .map(|v| v.id.clone());
    let refine = parent.is_some() && s.ask.preview && s.ask.view.as_ref().is_some_and(View::ready);
    let request = CreativeJobRequest {
        id: uuid(),
        expected_revision: e.mutation().document.revision,
        selected_ids: selected,
        prompt: draft,
        visual_asset_id: None,
        parent_job_id: parent,
        refine_proposal: refine,
    };
    s.ask.pending = Some(request.clone());
    s.ask.error = None;
    s.ask.preview = false;
    s.ask.busy = Some(s.enqueue(Kind::Ask(project, AskKind::Start(request))));
}

/// Resends a request whose reply was lost, with the same id.
pub(crate) fn retry(w: &World, s: &mut State) {
    let Some(project) = w
        .resource::<CanvasView>()
        .map(|v| v.editor.document_id().to_string())
    else {
        return;
    };
    if s.ask.sending() {
        return;
    }
    if let Some(request) = s.ask.pending.clone() {
        s.ask.busy = Some(s.enqueue(Kind::Ask(project, AskKind::Start(request))));
    } else if let (Some(decision), Some(view)) = (s.ask.decision.clone(), s.ask.view.clone()) {
        s.ask.busy = Some(s.enqueue(Kind::Ask(
            project,
            AskKind::Decide(view.source, view.id, decision),
        )));
    }
}

/// Uses or dismisses the current proposal, or cancels a running request.
pub(crate) fn decide(w: &World, s: &mut State, accept: bool) {
    let Some((project, revision)) = w.resource::<CanvasView>().map(|v| {
        (
            v.editor.document_id().to_string(),
            v.editor.mutation().document.revision,
        )
    }) else {
        return;
    };
    let Some(view) = s.ask.view.clone() else {
        return;
    };
    if s.ask.sending() {
        return;
    }
    let decision = s
        .ask
        .decision
        .clone()
        .filter(|d| d.accept == accept)
        .unwrap_or(CreativeProposalDecision {
            expected_revision: revision,
            accept,
        });
    s.ask.decision = Some(decision.clone());
    s.ask.error = None;
    s.ask.busy = Some(s.enqueue(Kind::Ask(
        project,
        AskKind::Decide(view.source, view.id, decision),
    )));
}

/// Reads the latest request when a project opens or Ask is shown, and checks
/// a running one every couple of seconds.
pub(crate) fn tick(w: &World, s: &mut State) {
    let Some(project) = w
        .resource::<CanvasView>()
        .map(|v| v.editor.document_id().to_string())
    else {
        return;
    };
    let due = s.ask.polled.is_none_or(|t| t.elapsed() >= JOB_POLL);
    if s.ask.executing() && s.ask.poll.is_none() && !s.ask.sending() && due {
        let id = s
            .ask
            .view
            .as_ref()
            .map(|v| v.id.clone())
            .unwrap_or_default();
        s.ask.polled = Some(std::time::Instant::now());
        s.ask.poll = Some(s.enqueue(Kind::Ask(project, AskKind::Job(id))));
    }
}

/// Reads the project's latest request and proposals.
pub(crate) fn refresh(w: &World, s: &mut State) {
    if s.ask.list.is_some() {
        return;
    }
    if let Some(project) = w
        .resource::<CanvasView>()
        .map(|v| v.editor.document_id().to_string())
    {
        s.ask.list = Some(s.enqueue(Kind::Ask(project, AskKind::Latest)));
    }
}

/// Shows a ready proposal over the canvas while Preview is chosen.
pub(crate) fn sync_preview(w: &mut World, s: &mut State) {
    let wanted = s
        .ask
        .view
        .as_ref()
        .filter(|v| s.ask.preview && v.ready())
        .map(|v| v.id.clone());
    if wanted == s.ask.shown {
        return;
    }
    let ops = s
        .ask
        .view
        .as_ref()
        .filter(|_| wanted.is_some())
        .map(|v| v.operations.clone());
    if let Some(v) = w.resource_mut::<CanvasView>() {
        v.editor.set_preview(ops);
    }
    s.ask.shown = wanted;
}

pub(crate) fn apply(
    w: &mut World,
    s: &mut State,
    serial: u64,
    project: &str,
    kind: AskKind,
    result: Result<AskReply, CanvasError>,
) {
    let current = w
        .resource::<CanvasView>()
        .is_some_and(|v| v.editor.document_id() == project);
    let slot = match kind {
        AskKind::Latest => &mut s.ask.list,
        AskKind::Job(_) => &mut s.ask.poll,
        AskKind::Start(_) | AskKind::Decide(..) => &mut s.ask.busy,
    };
    if *slot != Some(serial) {
        return;
    }
    *slot = None;
    if !current {
        return;
    }
    match (kind, result) {
        (AskKind::Latest, Ok(AskReply::Latest(view))) => {
            // A request sent meanwhile is newer than this read.
            if s.ask.pending.is_none() && s.ask.busy.is_none() {
                s.ask.view = view;
            }
        }
        (AskKind::Job(_), Ok(AskReply::Job(view))) => {
            if s.ask.view.as_ref().is_some_and(|v| v.id == view.id) {
                if view.ready() && !s.ask.view.as_ref().is_some_and(View::ready) {
                    // A fresh proposal opens on its preview, as in medousa-home.
                    s.ask.preview = true;
                }
                s.ask.view = Some(view);
            }
        }
        (AskKind::Start(_), Ok(AskReply::Job(view))) => {
            s.ask.pending = None;
            s.ask.draft.clear();
            s.ask.view = Some(view);
            s.ask.polled = Some(std::time::Instant::now());
        }
        (AskKind::Decide(..), Ok(AskReply::Decided(view, accepted))) => {
            s.ask.decision = None;
            s.ask.preview = false;
            match view {
                Some(view) => s.ask.view = Some(view),
                None => {
                    if let Some(v) = s.ask.view.as_mut() {
                        v.status = if accepted {
                            CreativeJobStatus::Accepted
                        } else {
                            CreativeJobStatus::Cancelled
                        };
                    }
                }
            }
            if accepted && let Some(v) = w.resource_mut::<CanvasView>() {
                // The daemon applied it; read the document it produced.
                let _ = v.editor.reload();
            }
        }
        (AskKind::Start(_) | AskKind::Decide(..), Err(e)) => {
            // A definite refusal never admitted work, so it is not retried.
            if matches!(
                e,
                CanvasError::Invalid(_) | CanvasError::Conflict(_) | CanvasError::NotFound
            ) {
                s.ask.pending = None;
                s.ask.decision = None;
            }
            s.ask.error = Some(e.to_string());
        }
        (_, Err(_)) | (_, Ok(_)) => {}
    }
}
