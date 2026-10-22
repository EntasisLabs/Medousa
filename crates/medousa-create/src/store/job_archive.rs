use super::*;

impl CreativeStore {
    pub(super) fn archived_job(&self, id: &str, job_id: &str) -> Result<Option<jobs::JobRecord>> {
        validate_id(id)?;
        validate_id(job_id)?;
        let file = path(&format!("job-archive/{id}/{job_id}.json"))?;
        if !self.root.is_file(&file).map_err(storage)? {
            return Ok(None);
        }
        let bytes = self
            .root
            .read_limited(&file, 8 * 1024 * 1024)
            .map_err(storage)?;
        let job: jobs::JobRecord = serde_json::from_slice(&bytes).map_err(storage)?;
        if job.job.document_id != id || job.job.request.id != job_id {
            return Err(storage("archived job identity mismatch"));
        }
        Ok(Some(job))
    }
    /// Keep the working record bounded without forgetting admission identities.
    pub(super) fn archive_finished_jobs(
        &self,
        id: &str,
        record: &mut Record,
        keep: Option<&str>,
    ) -> Result<()> {
        while record.jobs.len() > 32
            || serde_json::to_vec(&record.jobs).map_err(storage)?.len() > 6 * 1024 * 1024
        {
            let index = record
                .jobs
                .iter()
                .position(|j| {
                    Some(j.job.request.id.as_str()) != keep
                        && matches!(
                            j.job.status,
                            CreativeJobStatus::Accepted
                                | CreativeJobStatus::Cancelled
                                | CreativeJobStatus::Failed
                                | CreativeJobStatus::Interrupted
                                | CreativeJobStatus::Superseded
                        )
                })
                .ok_or_else(|| {
                    invalid(
                        "finish or dismiss an existing creative request before starting another",
                    )
                })?;
            let job = &record.jobs[index];
            self.root
                .create_dir_all(&path(&format!("job-archive/{id}"))?)
                .map_err(storage)?;
            self.root
                .atomic_write(
                    &path(&format!("job-archive/{id}/{}.json", job.job.request.id))?,
                    &serde_json::to_vec(job).map_err(storage)?,
                )
                .map_err(storage)?;
            record.jobs.remove(index);
        }
        Ok(())
    }
}
