//! Wire DTO and stream-transport bindings for declared operations.
//!
//! Names match `medousa-types` / types-schema titles. Unlisted operations
//! stay deferred until a DTO exists.

use medousa_api_contract::{RequestBodySpec, SchemaRef, StreamSpec, StreamTransport};

#[derive(Clone, Copy)]
pub(crate) struct WireBinding {
    pub request: Option<&'static str>,
    pub response: &'static str,
}

pub(crate) fn stream_binding(operation_id: &str) -> Option<(StreamTransport, &'static str)> {
    match operation_id {
        "code.lsp.get" | "grapheme.lsp.get" | "sessions.shell.by_id.get" => {
            Some((StreamTransport::WebSocket, "JsonRpcMessage"))
        }
        "interactive.turn.by_turn_id.stream.get"
        | "agents.sessions.by_agent_session_id.stream.get"
        | "ingest.by_stream_id.stream.get" => Some((StreamTransport::Sse, "TurnStreamEnvelopeV2")),
        "feeds.stream.get" => Some((StreamTransport::Sse, "FeedStreamEvent")),
        "environment.spec.stream.get" => Some((StreamTransport::Sse, "EnvironmentStreamEvent")),
        "workspace.stream.get" => Some((StreamTransport::Sse, "WorkspaceStreamEvent")),
        "local.models.download.by_job_id.events.get" => {
            Some((StreamTransport::Sse, "ModelDownloadProgress"))
        }
        "forge.stream.get" => Some((StreamTransport::Sse, "ForgeStreamEvent")),
        "forge.items.by_work_id.project_events.get"
        | "forge.items.by_work_id.task_runs.by_run_id.events.get" => {
            Some((StreamTransport::Sse, "ForgeProjectEvent"))
        }
        _ => None,
    }
}

pub(crate) fn stream_spec(transport: StreamTransport, item_name: &str) -> StreamSpec {
    let item = SchemaRef::named(item_name);
    match transport {
        StreamTransport::Sse => StreamSpec::json_events(item),
        StreamTransport::WebSocket => StreamSpec::websocket(item),
    }
}

pub(crate) fn wire_binding(operation_id: &str) -> Option<WireBinding> {
    Some(match operation_id {
        "liveness.get" => WireBinding {
            request: None,
            response: "HealthLiveness",
        },
        "health.get" => WireBinding {
            request: None,
            response: "HealthResponse",
        },
        "ingest.post" => WireBinding {
            request: Some("IngestRequest"),
            response: "IngestResponse",
        },
        "interactive.turn.post" => WireBinding {
            request: Some("InteractiveTurnRequest"),
            response: "InteractiveTurnResponse",
        },
        "vault.roots.get" => WireBinding {
            request: None,
            response: "VaultRootsResponse",
        },
        "vault.roots.post" => WireBinding {
            request: Some("VaultAddRootRequest"),
            response: "VaultRootsResponse",
        },
        "vault.active.put" => WireBinding {
            request: Some("VaultSetActiveRootRequest"),
            response: "VaultRootsResponse",
        },
        "vault.notes.get" => WireBinding {
            request: None,
            response: "VaultNotesListResponse",
        },
        "vault.notes.post" => WireBinding {
            request: Some("VaultWriteRequest"),
            response: "VaultWriteResponse",
        },
        "vault.notes.by_note_path.get" => WireBinding {
            request: None,
            response: "VaultNoteContentResponse",
        },
        "vault.notes.by_note_path.put" => WireBinding {
            request: Some("VaultWriteRequest"),
            response: "VaultWriteResponse",
        },
        "vault.notes.by_note_path.delete" => WireBinding {
            request: None,
            response: "VaultDeleteResponse",
        },
        "vault.tags.get" => WireBinding {
            request: None,
            response: "VaultTagsListResponse",
        },
        "vault.search.get" => WireBinding {
            request: None,
            response: "VaultSearchResponse",
        },
        "vault.backlinks.get" => WireBinding {
            request: None,
            response: "VaultBacklinksResponse",
        },
        "sessions.get" => WireBinding {
            request: None,
            response: "SessionHistoryListResponse",
        },
        "sessions.search.get" => WireBinding {
            request: None,
            response: "SessionTranscriptSearchResponse",
        },
        "sessions.derive.post" => WireBinding {
            request: Some("DeriveSessionRequest"),
            response: "DeriveSessionResponse",
        },
        "prompt_stashes.get" => WireBinding {
            request: None,
            response: "PromptStashListResponse",
        },
        "prompt_stashes.post" => WireBinding {
            request: Some("CreatePromptStashRequest"),
            response: "PromptStash",
        },
        "prompt_stashes.by_stash_id.delete" => WireBinding {
            request: None,
            response: "DeletePromptStashResponse",
        },
        "sessions.by_session_id.history.get" => WireBinding {
            request: None,
            response: "SessionHistoryResponse",
        },
        "sessions.by_session_id.turns.post" => WireBinding {
            request: Some("SessionAppendTurnRequest"),
            response: "SessionAppendTurnResponse",
        },
        "sessions.by_session_id.name.put" => WireBinding {
            request: Some("SessionSetDisplayNameRequest"),
            response: "SessionSetDisplayNameResponse",
        },
        "sessions.by_session_id.delete" => WireBinding {
            request: None,
            response: "SessionDeleteResponse",
        },
        "sessions.by_session_id.agent_mode.get" => WireBinding {
            request: None,
            response: "SessionAgentModeResponse",
        },
        "sessions.by_session_id.agent_mode.put" => WireBinding {
            request: Some("SetSessionAgentModeRequest"),
            response: "SessionAgentModeResponse",
        },
        "sessions.by_session_id.code_binding.get" => WireBinding {
            request: None,
            response: "SessionCodeBindingResponse",
        },
        "sessions.by_session_id.code_binding.put" => WireBinding {
            request: Some("SetSessionCodeBindingRequest"),
            response: "SessionCodeBindingResponse",
        },
        "sessions.by_session_id.code_project.post" => WireBinding {
            request: Some("StartSessionCodeProjectRequest"),
            response: "SessionCodeProjectResponse",
        },
        "calendar.events.get" => WireBinding {
            request: None,
            response: "CalendarListResponse",
        },
        "calendar.events.post" => WireBinding {
            request: Some("CalendarWriteRequest"),
            response: "CalendarWriteResponse",
        },
        "jobs.ask.post" => WireBinding {
            request: Some("EnqueueAskRequest"),
            response: "EnqueueResponse",
        },
        "jobs.report.post" => WireBinding {
            request: Some("EnqueueReportRequest"),
            response: "EnqueueResponse",
        },
        "jobs.prompt.post" => WireBinding {
            request: Some("EnqueuePromptRequest"),
            response: "EnqueueResponse",
        },
        "environment.spec.get" => WireBinding {
            request: None,
            response: "EnvironmentSpecResponse",
        },
        "environment.spec.put" => WireBinding {
            request: Some("EnvironmentSpecPutRequest"),
            response: "EnvironmentSpecResponse",
        },
        "environment.status.get" => WireBinding {
            request: None,
            response: "EnvironmentStatusResponse",
        },
        "environment.spec.validate.post" => WireBinding {
            request: Some("EnvironmentValidateRequest"),
            response: "EnvironmentValidateResponse",
        },
        "feeds.get" => WireBinding {
            request: None,
            response: "FeedListResponse",
        },
        "workspace.cards.get" => WireBinding {
            request: None,
            response: "WorkspaceCardsResponse",
        },
        "local.hardware.get" => WireBinding {
            request: None,
            response: "LocalHardwareResponse",
        },
        "local.catalog.get" => WireBinding {
            request: None,
            response: "LocalCatalogResponse",
        },
        "local.models.get" => WireBinding {
            request: None,
            response: "LocalModelsResponse",
        },
        "capabilities.get" => WireBinding {
            request: None,
            response: "CapabilityListResponse",
        },
        "mcp.gateway.status.get" => WireBinding {
            request: None,
            response: "McpGatewayStatusResponse",
        },
        "turns.budget_requests.get" => WireBinding {
            request: None,
            response: "TurnBudgetRequestListResponse",
        },
        "agents.runtimes.get" => WireBinding {
            request: None,
            response: "AgentRuntimeListResponse",
        },
        "agents.sessions.post" => WireBinding {
            request: Some("CreateAgentSessionRequest"),
            response: "CreateAgentSessionResponse",
        },
        "agents.secret_requests.get" => WireBinding {
            request: None,
            response: "AgentSecretRequestListResponse",
        },
        "agents.secret_requests.by_request_id.fulfill.post" => WireBinding {
            request: Some("AgentSecretFulfillRequest"),
            response: "AgentSecretResolveResponse",
        },
        "agents.secret_requests.by_request_id.deny.post" => WireBinding {
            request: Some("AgentSecretDenyRequest"),
            response: "AgentSecretResolveResponse",
        },
        other => {
            if let Some((_, name)) = stream_binding(other) {
                return Some(WireBinding {
                    request: None,
                    response: name,
                });
            }
            return None;
        }
    })
}

pub(crate) fn json_body(schema_name: &str) -> RequestBodySpec {
    RequestBodySpec {
        media_type: "application/json".into(),
        schema: SchemaRef::named(schema_name),
        required: true,
    }
}
