use super::*;
use serde_json::Value;

fn conflict() -> CreativeError {
    invalid(
        "Your edits and this result both changed the same part of an object. Your generation is still saved.",
    )
}
// Arrays (ink samples, etc.) are atomic. Object fields can merge independently,
// so moving an object never prevents an unrelated image/text/style replacement.
fn merge(
    base: Option<&Value>,
    current: Option<&Value>,
    proposed: Option<&Value>,
) -> Result<Option<Value>> {
    if proposed == base {
        return Ok(current.cloned());
    }
    if current == base || current == proposed {
        return Ok(proposed.cloned());
    }
    if let (
        Some(Value::Object(base)),
        Some(Value::Object(current)),
        Some(Value::Object(proposed)),
    ) = (base, current, proposed)
    {
        let keys: BTreeSet<_> = base
            .keys()
            .chain(current.keys())
            .chain(proposed.keys())
            .collect();
        let mut result = serde_json::Map::new();
        for key in keys {
            if let Some(value) = merge(base.get(key), current.get(key), proposed.get(key))? {
                result.insert(key.clone(), value);
            }
        }
        return Ok(Some(Value::Object(result)));
    }
    Err(conflict())
}

impl CreativeStore {
    /// Legacy refinements retain their original base in their parent chain.
    pub(super) fn job_base_nodes(
        &self,
        record: &Record,
        job: &CreativeJob,
    ) -> Result<Vec<CreativeNode>> {
        let mut current = job.clone();
        let mut seen = BTreeSet::new();
        loop {
            if let Some(nodes) = current.base_nodes {
                return Ok(nodes);
            }
            if !current.request.refine_proposal {
                return Ok(current.context.nodes);
            }
            if !seen.insert(current.request.id.clone()) {
                return Err(storage("creative parent cycle"));
            }
            let parent = current
                .request
                .parent_job_id
                .as_ref()
                .ok_or_else(conflict)?;
            let archived = self.archived_job(&job.document_id, parent)?;
            let previous = record
                .jobs
                .iter()
                .find(|j| &j.job.request.id == parent)
                .or(archived.as_ref())
                .ok_or_else(conflict)?;
            if previous.job.context.revision != job.context.revision {
                return Err(conflict());
            }
            current = previous.job.clone();
        }
    }
    pub(super) fn job_with_base(&self, record: &Record, job: &CreativeJob) -> Result<CreativeJob> {
        let mut job = job.clone();
        if job.base_nodes.is_none() {
            job.base_nodes = Some(self.job_base_nodes(record, &job)?);
        }
        Ok(job)
    }
    pub(super) fn reconcile_job(
        &self,
        record: &Record,
        job: &CreativeJob,
    ) -> Result<CreativeProposalRequest> {
        let mut proposal = job
            .result
            .clone()
            .ok_or_else(|| storage("ready job has no result"))?;
        if job.context.revision == record.document.revision {
            return Ok(proposal);
        }
        let base = self.job_base_nodes(record, job)?;
        let current = &record.document.scene.nodes;
        let mut operations = Vec::new();
        for operation in &proposal.operations {
            match operation {
                CreativeOperation::PutNode { node } => {
                    let old = base.iter().find(|n| n.id == node.id);
                    let now = current.iter().find(|n| n.id == node.id);
                    let old_json = old.map(serde_json::to_value).transpose().map_err(storage)?;
                    let now_json = now.map(serde_json::to_value).transpose().map_err(storage)?;
                    let next = merge(
                        old_json.as_ref(),
                        now_json.as_ref(),
                        Some(&serde_json::to_value(node).map_err(storage)?),
                    )?
                    .ok_or_else(conflict)?;
                    let next: CreativeNode = serde_json::from_value(next).map_err(storage)?;
                    if now != Some(&next) {
                        operations.push(CreativeOperation::PutNode { node: next });
                    }
                }
                CreativeOperation::RemoveNode { id } => {
                    if !current.iter().any(|n| &n.id == id) {
                        continue;
                    }
                    let mut subtree = BTreeSet::from([id.clone()]);
                    loop {
                        let before = subtree.len();
                        for node in current {
                            if node.parent_id.as_ref().is_some_and(|p| subtree.contains(p)) {
                                subtree.insert(node.id.clone());
                            }
                        }
                        if before == subtree.len() {
                            break;
                        }
                    }
                    if current
                        .iter()
                        .filter(|n| subtree.contains(&n.id))
                        .any(|n| base.iter().find(|old| old.id == n.id) != Some(n))
                    {
                        return Err(conflict());
                    }
                    operations.push(operation.clone());
                }
                _ => return Err(conflict()),
            }
        }
        proposal.operations = operations;
        proposal.expected_revision = record.document.revision;
        Ok(proposal)
    }
}
