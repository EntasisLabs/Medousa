//! Agent-facing trusted secret prompt for native Grapheme runs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::prelude::StasisError;

use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_GRAPHEME_REQUEST_SECRET: &str = "cognition_grapheme_request_secret";
const COGNITION_GRAPHEME_REQUEST_SECRET_ID: ToolId = ToolId::new(COGNITION_GRAPHEME_REQUEST_SECRET);

pub struct CognitionGraphemeRequestSecretTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionGraphemeRequestSecretTool {
    pub fn new(turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess) -> Self {
        Self { turn_scope }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphemeRequestSecretInput {
    /// Uppercase logical credential name, such as `GITHUB_TOKEN`.
    credential_key: String,
    /// Short trusted-UI label describing the credential.
    label: String,
    /// Why the Grapheme run needs the credential.
    reason: String,
    /// Exact HTTPS hosts (or host:port authorities) to which Medousa may attach
    /// this credential. Empty permits signing only and denies authenticated HTTP.
    #[serde(default)]
    allowed_hosts: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GraphemeRequestSecretOutput {
    Granted {
        grant_id: String,
        credential_key: String,
        allowed_hosts: Vec<String>,
        usage_hint: String,
    },
    Denied {
        reason: String,
    },
    Rejected {
        reason: String,
        policy_message: String,
    },
}

#[medousa_tool(id = COGNITION_GRAPHEME_REQUEST_SECRET_ID)]
impl CognitionGraphemeRequestSecretTool {
    /// Ask the user for a credential through Medousa's trusted UI for one native Grapheme run. The value never enters chat, Grapheme source/state, Stasis payloads, or model-visible output. The returned grant must be attached once to `cognition_capability` action `grapheme.invoke` via `secret_grant_ids`.
    async fn invoke_typed(
        &self,
        input: GraphemeRequestSecretInput,
    ) -> stasis::prelude::Result<GraphemeRequestSecretOutput> {
        let Some(scope) =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await
        else {
            return Ok(GraphemeRequestSecretOutput::Rejected {
                reason: "missing_turn_context".to_string(),
                policy_message: "secure credential requests require an active interactive turn"
                    .to_string(),
            });
        };
        if !scope.supports_ui_artifacts {
            return Ok(GraphemeRequestSecretOutput::Rejected {
                reason: "unsupported_surface".to_string(),
                policy_message:
                    "secure credential entry is available only on a trusted Medousa UI surface"
                        .to_string(),
            });
        }
        let Some(sink) = crate::engine_adapters::active_tool_sink().await else {
            return Ok(GraphemeRequestSecretOutput::Rejected {
                reason: "missing_trusted_prompt_channel".to_string(),
                policy_message: "the active turn cannot publish a trusted credential prompt"
                    .to_string(),
            });
        };
        let allowed_hosts =
            crate::agent_secret_request::normalize_allowed_hosts(&input.allowed_hosts)
                .map_err(StasisError::PortFailure)?;
        let record = crate::agent_secret_request::agent_secret_request_store()
            .create(crate::agent_secret_request::CreateAgentSecretRequest {
                turn_id: scope.turn_correlation_id,
                session_id: scope.session_id,
                provider_type: "grapheme".to_string(),
                credential_key: input.credential_key.clone(),
                backend: medousa_types::AgentSecretRequestBackend::GraphemeRuntime,
                allowed_hosts,
                label: input.label,
                reason: input.reason,
            })
            .map_err(StasisError::PortFailure)?;

        sink.emit(medousa_engine::ToolSinkEvent::SecretRequest {
            request_id: record.request_id.clone(),
            label: record.label,
            reason: record.reason,
            provider_type: record.provider_type.clone(),
            credential_key: record.credential_key.clone(),
            backend: "grapheme_runtime".to_string(),
            allowed_hosts: record.allowed_hosts.clone(),
        })
        .await;

        match crate::agent_secret_request::agent_secret_request_store()
            .wait_for_resolution(&record.request_id)
            .await
            .map_err(StasisError::PortFailure)?
        {
            crate::agent_secret_request::SecretRequestResolution::Granted { grant_id } => {
                Ok(GraphemeRequestSecretOutput::Granted {
                    grant_id,
                    credential_key: record.credential_key,
                    allowed_hosts: record.allowed_hosts,
                    usage_hint: "Give the opaque grant to the Workshop. Attach it once as grapheme.invoke.secret_grant_ids; in source call secrets.get_secret_handle(name: grant_id), then secrets.sign_request or medousa.authorized_http. Never print or persist the grant."
                        .to_string(),
                })
            }
            crate::agent_secret_request::SecretRequestResolution::Denied => {
                Ok(GraphemeRequestSecretOutput::Denied {
                    reason: "the user denied or did not complete secure credential entry"
                        .to_string(),
                })
            }
        }
    }
}

pub fn register_grapheme_secret_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionGraphemeRequestSecretTool::new(turn_scope))?;
    Ok(())
}
