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
            || request.existing_agent_session_id.is_some()
            || turn_id != format!("medousa_coder_{}", request.assignment_id)
        {
            bail!("invalid native Coder correlation");
        }
        self.require_assignment_grant(request, chrono::Utc::now())?;
        let path = object_path(
            &Self::native_coder_scope(&request.target.authority_id),
            "native-coder",
            turn_id,
        )?;
        self.create(&path, request)?;
        self.root.sync_parent_of(&path)?;
        Ok(())
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
