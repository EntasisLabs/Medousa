//! The project lock is also the job/result fence. A job can never apply edits
//! from a callback; only an explicit decision uses the normal undoable edit path.
use super::*;

const MAX_JOB_BYTES: usize = 8 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESULT_BYTES: usize = 512 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct JobRecord {
    pub(super) job: CreativeJob,
    owner: String,
    /// Retained only during execution, for validation against the admitted base.
    pub(super) snapshot: Option<CreativeDocument>,
    #[serde(default)]
    inherited: Vec<CreativeOperation>,
    #[serde(default)]
    result_input: Option<String>,
}

/// Small recovery/outbox descriptor: no scenes, patches or bitmap payloads.
#[derive(Clone, Debug)]
pub struct CreativeDispatch {
    pub document_id: String,
    pub job_id: String,
    pub owner: String,
    pub turn_id: String,
    pub session_id: String,
    pub status: CreativeJobStatus,
}

impl CreativeStore {
    fn job_index(turn_id: &str) -> Result<StorePath> {
        validate_id(turn_id)?;
        path(&format!("jobs/{turn_id}.json"))
    }
    fn publish_job_index(&self, id: &str, job: &CreativeJob) -> Result<()> {
        self.root
            .atomic_write(
                &Self::job_index(&job.turn_id)?,
                &serde_json::to_vec(&(id, &job.request.id)).map_err(storage)?,
            )
            .map_err(storage)
    }
    fn outbox_path(turn_id: &str) -> Result<StorePath> {
        validate_id(turn_id)?;
        path(&format!("outbox/{turn_id}.json"))
    }
    fn publish_outbox(&self, id: &str, job: &CreativeJob) -> Result<()> {
        self.root
            .atomic_write(
                &Self::outbox_path(&job.turn_id)?,
                &serde_json::to_vec(&(id, &job.request.id)).map_err(storage)?,
            )
            .map_err(storage)
    }
    fn job_position(record: &Record, job_id: &str) -> Result<usize> {
        validate_id(job_id)?;
        record
            .jobs
            .iter()
            .position(|j| j.job.request.id == job_id)
            .ok_or(CreativeError::NotFound)
    }
    fn job_budget(record: &Record) -> Result<()> {
        if serde_json::to_vec(&record.jobs).map_err(storage)?.len() > MAX_JOB_BYTES {
            return Err(invalid("creative job byte capacity reached"));
        }
        Ok(())
    }
    pub fn enqueue_job(
        &self,
        id: &str,
        owner: &str,
        request: CreativeJobRequest,
    ) -> Result<CreativeJob> {
        validate_id(&request.id)?;
        bounded_text(owner, 256, "job owner")?;
        bounded_text(&request.prompt, 16 * 1024, "creative prompt")?;
        if owner.trim().is_empty() || request.prompt.trim().is_empty() {
            return Err(invalid("job requires an owner and prompt"));
        }
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        if let Some(existing) = record.jobs.iter().find(|j| j.job.request.id == request.id) {
            if existing.owner != owner || existing.job.request != request {
                return Err(invalid(
                    "creative job id reused with different input or owner",
                ));
            }
            self.publish_job_index(id, &existing.job)?;
            if existing.job.status == CreativeJobStatus::Queued || existing.job.status.executing() {
                self.publish_outbox(id, &existing.job)?;
            }
            return Ok(existing.job.clone());
        }
        if let Some(existing) = self.archived_job(id, &request.id)? {
            if existing.owner != owner || existing.job.request != request {
                return Err(invalid(
                    "creative job id reused with different input or owner",
                ));
            }
            return Ok(existing.job);
        }
        Self::check_revision(&record, request.expected_revision)?;
        if serde_json::to_vec(&record.document).map_err(storage)?.len() > MAX_SNAPSHOT_BYTES {
            return Err(invalid("creative job base exceeds 2 MiB"));
        }
        let mut snapshot = record.document.clone();
        let mut inherited = vec![];
        let mut inherited_assets = vec![];
        let session_id = if let Some(parent) = &request.parent_job_id {
            let archived = self.archived_job(id, parent)?;
            let previous = record
                .jobs
                .iter()
                .find(|j| &j.job.request.id == parent)
                .or(archived.as_ref())
                .ok_or(CreativeError::NotFound)?;
            if previous.owner != owner {
                return Err(CreativeError::NotFound);
            }
            if previous.job.status.executing() || previous.job.status == CreativeJobStatus::Queued {
                return Err(invalid("finish the previous creative request first"));
            }
            if record.jobs.iter().any(|j| {
                j.job.session_id == previous.job.session_id
                    && (j.job.status.executing() || j.job.status == CreativeJobStatus::Queued)
            }) {
                return Err(invalid(
                    "this creative conversation already has an active request",
                ));
            }
            if request.refine_proposal {
                if previous.job.status != CreativeJobStatus::Ready
                    || previous.job.context.revision != request.expected_revision
                    || previous.job.request.selected_ids != request.selected_ids
                {
                    return Err(invalid(
                        "refine a current preview with its original selection",
                    ));
                }
                inherited = previous
                    .job
                    .result
                    .as_ref()
                    .ok_or_else(|| invalid("preview has no proposal"))?
                    .operations
                    .clone();
                inherited_assets = previous.job.assets.clone();
                for asset in &inherited_assets {
                    snapshot.assets.insert(asset.id.clone(), asset.clone());
                }
            }
            previous.job.session_id.clone()
        } else {
            if request.refine_proposal {
                return Err(invalid("refining requires a previous creative request"));
            }
            format!("ses_create_{}", hash(&(id, &request.id))?)
        };
        let effective = if inherited.is_empty() {
            snapshot.clone()
        } else {
            apply_operations(&snapshot, &inherited)?
        };
        let context = Self::selection_context(
            &effective,
            CreativeContextRequest {
                selected_ids: request.selected_ids.clone(),
            },
        )?;
        let visual_context = if let Some(asset_id) = &request.visual_asset_id {
            let visual = record
                .visuals
                .get(asset_id)
                .ok_or_else(|| invalid("capture the selection preview again"))?;
            let original = Self::selection_context(
                &record.document,
                CreativeContextRequest {
                    selected_ids: request.selected_ids.clone(),
                },
            )?;
            if visual.revision != request.expected_revision
                || visual.selected_ids != request.selected_ids
                || visual.fingerprint != original.fingerprint
            {
                return Err(invalid("selection preview does not match this request"));
            }
            Some(visual.asset.clone())
        } else {
            None
        };
        if context.assets.len() + usize::from(visual_context.is_some()) > 5 {
            return Err(invalid(
                "select at most four distinct images with a selection preview",
            ));
        }
        let identity = hash(&(id, &request.id))?;
        if let Some(parent) = &request.parent_job_id {
            if let Some(previous) = record.jobs.iter_mut().find(|j| &j.job.request.id == parent) {
                if matches!(
                    previous.job.status,
                    CreativeJobStatus::Ready | CreativeJobStatus::Stale
                ) {
                    previous.job.status = CreativeJobStatus::Superseded;
                    previous.job.updated_at = now();
                }
            }
        }
        let time = now();
        let job = CreativeJob {
            request,
            document_id: id.into(),
            context,
            status: CreativeJobStatus::Queued,
            turn_id: format!("creative_job_{identity}"),
            session_id,
            created_at: time.clone(),
            updated_at: time,
            result: None,
            assets: inherited_assets,
            visual_context,
            error: None,
            applied_revision: None,
        };
        record.jobs.push(JobRecord {
            job: job.clone(),
            owner: owner.into(),
            snapshot: Some(snapshot),
            inherited,
            result_input: None,
        });
        self.archive_finished_jobs(id, &mut record, Some(&job.request.id))?;
        if let Some(asset_id) = &job.request.visual_asset_id {
            record.visuals.remove(asset_id);
        }
        Self::job_budget(&record)?;
        // Publishing the locator first is safe: lookup also requires the atomic
        // project admission. An orphan locator cannot authorize an execution.
        self.publish_job_index(id, &job)?;
        self.publish_outbox(id, &job)?;
        self.write(&record)?;
        Ok(job)
    }
    pub fn job(&self, id: &str, job_id: &str, owner: &str) -> Result<CreativeJob> {
        let record = self.read_record(id, true)?;
        let archived = self.archived_job(id, job_id)?;
        let job = record
            .jobs
            .iter()
            .find(|j| j.job.request.id == job_id)
            .or(archived.as_ref())
            .ok_or(CreativeError::NotFound)?;
        if job.owner != owner {
            return Err(CreativeError::NotFound);
        }
        Ok(job.job.clone())
    }
    pub fn jobs(&self, id: &str, owner: &str) -> Result<CreativeJobListResponse> {
        Ok(CreativeJobListResponse {
            jobs: self
                .read_record(id, true)?
                .jobs
                .into_iter()
                .filter(|j| j.owner == owner)
                .map(|j| j.job)
                .collect(),
        })
    }
    pub fn job_for_turn(&self, turn_id: &str) -> Result<CreativeDispatch> {
        let bytes = self
            .root
            .read_limited(&Self::job_index(turn_id)?, 1024)
            .map_err(storage)?;
        let (id, job_id): (String, String) = serde_json::from_slice(&bytes).map_err(storage)?;
        let record = self.read_record(&id, true)?;
        let archived = self.archived_job(&id, &job_id)?;
        let job = record
            .jobs
            .iter()
            .find(|j| j.job.request.id == job_id)
            .or(archived.as_ref())
            .ok_or(CreativeError::NotFound)?;
        if job.job.turn_id != turn_id {
            return Err(invalid("job execution identity mismatch"));
        }
        Ok(Self::dispatch(&id, job))
    }
    fn dispatch(id: &str, record: &JobRecord) -> CreativeDispatch {
        CreativeDispatch {
            document_id: id.into(),
            job_id: record.job.request.id.clone(),
            owner: record.owner.clone(),
            turn_id: record.job.turn_id.clone(),
            session_id: record.job.session_id.clone(),
            status: record.job.status,
        }
    }
    /// Read only pending locators, never scan idle documents. Holding the
    /// project lock resolves the publish-before-admit crash window: an orphan
    /// can be removed only after a concurrent admission has finished.
    pub fn job_dispatch_page(
        &self,
        cursor: Option<&str>,
    ) -> Result<(Vec<CreativeDispatch>, Option<String>, Vec<String>)> {
        let mut turns: Vec<_> = self
            .root
            .list_directory(&path("outbox")?)
            .map_err(storage)?
            .into_iter()
            .filter(|e| e.kind == StoreEntryKind::File)
            .filter_map(|e| e.path.file_name().strip_suffix(".json").map(str::to_string))
            .filter(|turn| cursor.is_none_or(|c| turn.as_str() > c))
            .collect();
        turns.sort();
        let more = turns.len() > 16;
        turns.truncate(16);
        let next = more.then(|| turns.last().expect("nonempty page").clone());
        let mut jobs = Vec::new();
        let mut errors = Vec::new();
        for turn in turns {
            match self.pending_dispatch(&turn) {
                Ok(Some(job)) => jobs.push(job),
                Ok(None) => {}
                Err(error) => errors.push(format!("{turn}: {error}")),
            }
        }
        Ok((jobs, next, errors))
    }
    fn pending_dispatch(&self, turn: &str) -> Result<Option<CreativeDispatch>> {
        let outbox = Self::outbox_path(turn)?;
        let bytes = self.root.read_limited(&outbox, 1024).map_err(storage)?;
        let (id, job_id): (String, String) = serde_json::from_slice(&bytes).map_err(storage)?;
        let _lock = self.lock(&id)?;
        let record = match self.read_record(&id, true) {
            Ok(record) => record,
            Err(CreativeError::NotFound) => {
                self.root.remove_file(&outbox).map_err(storage)?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let job = match record.jobs.iter().find(|j| j.job.request.id == job_id) {
            Some(job) => job,
            None => {
                self.root.remove_file(&outbox).map_err(storage)?;
                return Ok(None);
            }
        };
        if job.job.turn_id != turn {
            return Err(invalid("outbox execution identity mismatch"));
        }
        if job.job.status != CreativeJobStatus::Queued
            && !job.job.status.executing()
            && job.job.status != CreativeJobStatus::Cancelled
        {
            self.root.remove_file(&outbox).map_err(storage)?;
            return Ok(None);
        }
        if !self
            .root
            .is_file(&Self::job_index(turn)?)
            .map_err(storage)?
        {
            self.publish_job_index(&id, &job.job)?;
        }
        Ok(Some(Self::dispatch(&id, job)))
    }
    pub fn acknowledge_job_cancellation(&self, id: &str, job_id: &str) -> Result<()> {
        let _lock = self.lock(id)?;
        let record = self.read_record(id, true)?;
        let index = Self::job_position(&record, job_id)?;
        let job = &record.jobs[index].job;
        if job.status == CreativeJobStatus::Cancelled {
            self.root
                .remove_file(&Self::outbox_path(&job.turn_id)?)
                .map_err(storage)?;
        }
        Ok(())
    }
    /// Durable dispatch fence before calling the existing runtime. A restart
    /// never turns Dispatching back into Queued without proof of non-admission.
    pub fn claim_job(&self, id: &str, job_id: &str) -> Result<Option<CreativeJob>> {
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        let index = Self::job_position(&record, job_id)?;
        let job = &mut record.jobs[index].job;
        if job.status != CreativeJobStatus::Queued {
            return Ok(None);
        }
        job.status = CreativeJobStatus::Dispatching;
        job.error = None;
        job.updated_at = now();
        let result = job.clone();
        self.write(&record)?;
        Ok(Some(result))
    }
    pub fn mark_job_running(&self, id: &str, job_id: &str) -> Result<()> {
        let _lock = self.lock(id)?;
        let mut record = self.read_record(id, true)?;
        let index = Self::job_position(&record, job_id)?;
        let job = &mut record.jobs[index].job;
        if job.status == CreativeJobStatus::Dispatching {
            job.status = CreativeJobStatus::Running;
            job.updated_at = now();
            self.write(&record)?;
        }
        Ok(())
    }
    /// Only a synchronous spawn rejection is proof that retry is safe.
    pub fn reject_job_dispatch(
        &self,
        id: &str,
        job_id: &str,
        retry: bool,
        reason: String,
    ) -> Result<()> {
        let _lock = self.lock(id)?;
        let mut record = self.read_record(id, true)?;
        let index = Self::job_position(&record, job_id)?;
        let job = &mut record.jobs[index];
        if job.job.status != CreativeJobStatus::Dispatching {
            return Ok(());
        }
        job.job.status = if retry {
            CreativeJobStatus::Queued
        } else {
            CreativeJobStatus::Failed
        };
        job.job.error = Some(reason.chars().take(2000).collect());
        job.job.updated_at = now();
        if !retry {
            job.snapshot = None;
        }
        self.write(&record)
    }
    pub fn observe_job_turn(&self, id: &str, job_id: &str, phase: &str) -> Result<()> {
        self.observe_job_outcome(id, job_id, phase, None)
    }
    /// Preserve safe runtime failure copy before terminal phase reconciliation.
    /// Ready proposals and cancelled jobs remain fenced against late failures.
    pub fn observe_job_failure(&self, id: &str, job_id: &str, message: &str) -> Result<()> {
        self.observe_job_outcome(id, job_id, "error", Some(message))
    }
    fn observe_job_outcome(
        &self,
        id: &str,
        job_id: &str,
        phase: &str,
        failure: Option<&str>,
    ) -> Result<()> {
        let _lock = self.lock(id)?;
        let mut record = self.read_record(id, true)?;
        let index = Self::job_position(&record, job_id)?;
        let job = &mut record.jobs[index];
        if !job.job.status.executing() {
            return Ok(());
        }
        let (status, message) = match phase {
            "done" => (
                CreativeJobStatus::Failed,
                "Turn completed without a creative proposal.",
            ),
            "error" => (
                CreativeJobStatus::Failed,
                "The creative turn failed before returning a proposal.",
            ),
            "cancelled" => (
                CreativeJobStatus::Cancelled,
                "The creative turn was cancelled.",
            ),
            "interrupted" => (
                CreativeJobStatus::Interrupted,
                "Execution was interrupted. Submit a new job to retry; the previous execution will not be replayed.",
            ),
            _ => return Ok(()),
        };
        job.job.status = status;
        job.job.error = Some(failure.unwrap_or(message).chars().take(2000).collect());
        job.job.updated_at = now();
        job.snapshot = None;
        self.write(&record)
    }
    pub fn job_context(
        &self,
        id: &str,
        job_id: &str,
        turn_id: &str,
    ) -> Result<CreativeSelectionContext> {
        let record = self.read(id)?;
        let index = Self::job_position(&record, job_id)?;
        let job = &record.jobs[index].job;
        if job.turn_id != turn_id || !job.status.executing() {
            return Err(invalid("creative job is no longer executing"));
        }
        Ok(job.context.clone())
    }
    pub fn complete_job(
        &self,
        id: &str,
        job_id: &str,
        turn_id: &str,
        mut request: CreativeProposalRequest,
    ) -> Result<CreativeJob> {
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        let index = Self::job_position(&record, job_id)?;
        let job = &record.jobs[index];
        let input_digest = hash(&request)?;
        if job.job.turn_id != turn_id {
            return Err(invalid("wrong creative execution"));
        }
        if job.job.status != CreativeJobStatus::Cancelled
            && job.result_input.as_ref() == Some(&input_digest)
        {
            return Ok(job.job.clone());
        }
        // A refinement is returned as a complete replacement proposal against
        // the unchanged human revision, so acceptance remains one undoable edit.
        if !job.inherited.is_empty() {
            if let Some(snapshot) = &job.snapshot {
                let effective = apply_operations(snapshot, &job.inherited)?;
                Self::validate_proposal(&effective, &request)?;
                let next = apply_operations(&effective, &request.operations)?;
                request.operations = proposal_delta(snapshot, &next);
            }
        }
        if let Some(result) = &job.job.result {
            if job.job.status != CreativeJobStatus::Cancelled && hash(result)? == hash(&request)? {
                return Ok(job.job.clone());
            }
            return Err(invalid("creative result is already terminal"));
        }
        if !job.job.status.executing() {
            return Err(invalid("creative job is no longer executing"));
        }
        if request.id != job_id
            || request.expected_revision != job.job.context.revision
            || request.selected_ids != job.job.request.selected_ids
        {
            return Err(invalid(
                "proposal must use the admitted job ID, revision and selection",
            ));
        }
        if serde_json::to_vec(&request).map_err(storage)?.len() > MAX_RESULT_BYTES {
            return Err(invalid("creative result exceeds 512 KiB"));
        }
        let snapshot = job
            .snapshot
            .as_ref()
            .ok_or_else(|| storage("missing job base"))?;
        Self::validate_proposal(snapshot, &request)?;
        let current = record.document.revision;
        let job = &mut record.jobs[index];
        job.result_input = Some(input_digest);
        job.job.status = if current == request.expected_revision {
            CreativeJobStatus::Ready
        } else {
            CreativeJobStatus::Stale
        };
        job.job.result = Some(request);
        job.job.error = None;
        job.job.updated_at = now();
        job.snapshot = None;
        Self::job_budget(&record)?;
        let result = record.jobs[index].job.clone();
        self.write(&record)?;
        Ok(result)
    }
    pub fn decide_job(
        &self,
        id: &str,
        job_id: &str,
        owner: &str,
        request: CreativeProposalDecision,
    ) -> Result<CreativeJob> {
        let _lock = self.lock(id)?;
        let mut record = self.read_record(id, true)?;
        if !record.jobs.iter().any(|j| j.job.request.id == job_id) {
            let archived = self
                .archived_job(id, job_id)?
                .ok_or(CreativeError::NotFound)?;
            if archived.owner != owner {
                return Err(CreativeError::NotFound);
            }
            if (request.accept && archived.job.status == CreativeJobStatus::Accepted)
                || (!request.accept && archived.job.status == CreativeJobStatus::Cancelled)
            {
                return Ok(archived.job);
            }
            return Err(invalid(
                "this creative request is archived; start a follow-up",
            ));
        }
        let index = Self::job_position(&record, job_id)?;
        if record.jobs[index].owner != owner {
            return Err(CreativeError::NotFound);
        }
        let job = &record.jobs[index].job;
        if request.accept && job.status == CreativeJobStatus::Accepted {
            return Ok(job.clone());
        }
        if !request.accept && job.status == CreativeJobStatus::Cancelled {
            return Ok(job.clone());
        }
        if !request.accept {
            if job.status == CreativeJobStatus::Accepted {
                return Err(invalid("accepted job cannot be cancelled; use undo"));
            }
            record.jobs[index].job.status = CreativeJobStatus::Cancelled;
            record.jobs[index].snapshot = None;
        } else {
            if record.deleted {
                return Err(CreativeError::NotFound);
            }
            if job.status != CreativeJobStatus::Ready {
                return Err(invalid(
                    "creative result is not ready; stale results require a new job",
                ));
            }
            Self::check_revision(&record, request.expected_revision)?;
            Self::check_revision(&record, job.context.revision)?;
            let result = job
                .result
                .clone()
                .ok_or_else(|| storage("ready job has no result"))?;
            // Assets become part of the document in the same atomic edit, so
            // generation itself does not advance or invalidate the human base.
            for asset in job.assets.clone() {
                record.document.assets.insert(asset.id.clone(), asset);
            }
            let transaction = CreativeTransaction {
                id: format!("job_{}", hash(&(id, job_id))?),
                expected_revision: request.expected_revision,
                label: result.label,
                operations: result.operations,
            };
            let digest = hash(&transaction)?;
            Self::edit(&mut record, &transaction, digest)?;
            record.jobs[index].job.status = CreativeJobStatus::Accepted;
            record.jobs[index].job.applied_revision = Some(record.document.revision);
        }
        record.jobs[index].job.updated_at = now();
        let result = record.jobs[index].job.clone();
        self.write(&record)?;
        Ok(result)
    }
    pub fn import_job_asset(
        &self,
        id: &str,
        job_id: &str,
        turn_id: &str,
        bytes: &[u8],
    ) -> Result<CreativeAsset> {
        let asset = Self::bitmap_asset(bytes)?;
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        let index = Self::job_position(&record, job_id)?;
        let job = &mut record.jobs[index];
        if job.job.turn_id != turn_id || !job.job.status.executing() {
            return Err(invalid("creative job is no longer executing"));
        }
        if job.job.assets.iter().any(|a| a.id == asset.id) {
            self.publish_asset(id, &asset, bytes)?;
            return Ok(asset);
        }
        if job.job.assets.len() >= 16 {
            return Err(invalid("creative job asset capacity reached"));
        }
        let snapshot = job
            .snapshot
            .as_mut()
            .ok_or_else(|| storage("missing job base"))?;
        snapshot.assets.insert(asset.id.clone(), asset.clone());
        validate_document(snapshot)?;
        job.job.assets.push(asset.clone());
        let mut all_assets = record.document.clone();
        for staged in record.jobs.iter().flat_map(|j| &j.job.assets) {
            all_assets.assets.insert(staged.id.clone(), staged.clone());
        }
        validate_document(&all_assets)?;
        Self::job_budget(&record)?;
        self.publish_asset(id, &asset, bytes)?;
        self.write(&record)?;
        Ok(asset)
    }
}

fn proposal_delta(from: &CreativeDocument, to: &CreativeDocument) -> Vec<CreativeOperation> {
    let old: BTreeMap<_, _> = from.scene.nodes.iter().map(|n| (&n.id, n)).collect();
    let ids: BTreeSet<_> = to.scene.nodes.iter().map(|n| &n.id).collect();
    let mut operations: Vec<_> = from
        .scene
        .nodes
        .iter()
        .filter(|n| !ids.contains(&n.id))
        .map(|n| CreativeOperation::RemoveNode { id: n.id.clone() })
        .collect();
    operations.extend(
        to.scene
            .nodes
            .iter()
            .filter(|n| old.get(&n.id).copied() != Some(*n))
            .map(|n| CreativeOperation::PutNode { node: n.clone() }),
    );
    operations
}
