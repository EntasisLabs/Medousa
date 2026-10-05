//! Publish the same native freshness and memory lifecycle as project UI actions.
use super::*;
impl LocalPeerDispatcher {
    pub(crate) async fn finalize_project_lifecycle(
        &self,
        item: &medousa_forge::model::WorkItem,
        decision_id: Option<&medousa_forge::model::ReviewDecisionId>,
    ) -> serde_json::Value {
        self.state.forge_events.publish(
            item.id.as_str(),
            &item.state.to_string(),
            "assistant_lifecycle",
        );
        if !item.state.is_terminal() {
            return serde_json::json!({"terminal": false});
        }
        if item.state == medousa_forge::model::WorkState::Discarded {
            let id = item.id.clone();
            let cleanup = self.state.forge_execution.run(ExecutionClass::StoreIo, 64 * 1024, move || {
                for session_id in crate::agent_mode_state::session_ids_with_code_binding() {
                    let bound = crate::agent_mode_state::get_session_code_binding(&session_id)
                        .is_ok_and(|binding| binding.work_id.as_deref() == Some(id.as_str()));
                    if bound && let Err(error) = crate::agent_mode_state::clear_session_code_binding(&session_id) {
                        tracing::warn!(work_id = %id, session_id, %error, "failed to clear discarded undertaking binding");
                    }
                }
                Ok(())
            }).await;
            if let Err(error) = cleanup {
                tracing::warn!(work_id = %item.id, %error, "discarded undertaking binding cleanup deferred");
            }
        }
        crate::agent_runtime::coder_tools::finalize_coder_memory_lineage(
            self.state.platform.agent().tool_registry.clone(),
            self.state.forge.clone(),
            item,
            decision_id,
        )
        .await
    }
}
