//! Native turn correlation. Claims and exact grants still own admission.
use super::*;
use medousa_types::{AuthorityId, coordination::ExternalPeerRuntime};

impl CoordinationStore {
    fn native_coder_scope(authority: &AuthorityId) -> CoordinationChannelRef {
        CoordinationChannelRef {
            authority_id: authority.clone(),
            channel_id: "native-coder-turns".into(),
        }
    }

    pub fn record_native_coder(
        &self,
        turn_id: &str,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<()> {
        if request.target.runtime != ExternalPeerRuntime::Medousa
            || turn_id != format!("medousa_coder_{}", request.assignment_id)
        {
            bail!("invalid native Coder correlation");
        }
        self.require_assignment_grant(request, chrono::Utc::now())?;
        // A session, unlike an assignment, can have many sequential turns. Fence
        // competing follow-ups across processes before publishing correlation.
        let scope = Self::native_coder_scope(&request.target.authority_id);
        let _session =
            self.work_assignment_lock(&scope, request.execution_session.session_id.as_str())?;
        if let Some(previous) = request.existing_agent_session_id.as_deref() {
            let prior = self.native_coder_reuse(request, previous)?;
            if request.execution_session != prior.execution_session {
                bail!("native Coder continuation changed its execution session");
            }
        }
        for active in
            self.native_coder_sessions(&request.target.authority_id, &request.owner_principal_id)?
        {
            if active.execution_session == request.execution_session
                && active.assignment_id != request.assignment_id
                && self
                    .receipt_if_recorded(&active.channel, &active.assignment_id)?
                    .is_none()
            {
                bail!("native Coder session already has an unresolved assignment");
            }
        }
        let path = object_path(
            &Self::native_coder_scope(&request.target.authority_id),
            "native-coder",
            turn_id,
        )?;
        self.create(&path, request)?;
        self.root.sync_parent_of(&path)?;
        Ok(())
    }

    /// Durable discovery, including completed sessions after daemon restart.
    pub fn native_coder_sessions(
        &self,
        authority: &AuthorityId,
        owner: &str,
    ) -> Result<Vec<ExternalPeerAssignmentRequest>> {
        let entries = self.root.list_root_utf8()?;
        if entries.len() > 10_000 {
            bail!("native Coder discovery scan budget exhausted");
        }
        let mut requests = Vec::new();
        for entry in entries {
            if !entry.name.starts_with("nc1-") {
                continue;
            }
            let request: ExternalPeerAssignmentRequest =
                self.read(&StorePath::parse(&entry.name)?)?;
            if request.owner_principal_id == owner && request.target.authority_id == *authority {
                let turn_id = format!("medousa_coder_{}", request.assignment_id);
                if object_path(
                    &Self::native_coder_scope(authority),
                    "native-coder",
                    &turn_id,
                )?
                .file_name()
                    != entry.name
                {
                    bail!("native Coder discovery identity mismatch");
                }
                self.require_owner(&request.channel, owner)?;
                requests.push(request);
            }
        }
        Ok(requests)
    }

    /// Reuse conversation context, never the old execution grant. Only the
    /// originating sender in the same project/channel/runtime can continue it.
    pub fn native_coder_reuse(
        &self,
        next: &ExternalPeerAssignmentRequest,
        previous_turn: &str,
    ) -> Result<ExternalPeerAssignmentRequest> {
        let previous = self.native_coder_request(&next.target.authority_id, previous_turn)?;
        if previous.owner_principal_id != next.owner_principal_id
            || previous.owner_session != next.owner_session
            || previous.channel != next.channel
            || previous.forge_work_id != next.forge_work_id
            || previous.target != next.target
            || previous.assignment_id == next.assignment_id
        {
            bail!("native Coder continuation must retain its owner, sender, project and runtime");
        }
        self.require_owner(&previous.channel, &next.owner_principal_id)?;
        let binding = self.peer(&previous.channel, &previous.assignment_id)?;
        if binding.agent_session_id != previous_turn
            || self
                .receipt_if_recorded(&previous.channel, &previous.assignment_id)?
                .is_none()
        {
            bail!("native Coder continuation requires a terminal assignment receipt");
        }
        Ok(previous)
    }

    pub fn native_coder_request(
        &self,
        authority: &AuthorityId,
        turn_id: &str,
    ) -> Result<ExternalPeerAssignmentRequest> {
        let request: ExternalPeerAssignmentRequest = self.read(&object_path(
            &Self::native_coder_scope(authority),
            "native-coder",
            turn_id,
        )?)?;
        if request.target.authority_id != *authority
            || request.target.runtime != ExternalPeerRuntime::Medousa
            || turn_id != format!("medousa_coder_{}", request.assignment_id)
        {
            bail!("native Coder correlation mismatch");
        }
        Ok(request)
    }
}
