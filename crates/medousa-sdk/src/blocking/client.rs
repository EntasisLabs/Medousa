#[cfg(feature = "blocking")]
use medousa_types::{
    ActiveSessionTurnResponse, AgentModeListResponse, AgentModeProposalListResponse,
    AgentModeProposalResponse, AgentModeScope, AgentModeTransitionPolicy, ArchiveAskJobRequest,
    ArchiveAskJobResponse, ArtifactCommandRequest, ArtifactCommandResponse, ArtifactDeleteRequest,
    ArtifactDeleteResponse, ArtifactFetchRequest, ArtifactFetchResponse, ArtifactListUiRequest,
    ArtifactListUiResponse, ArtifactWriteRequest, ArtifactWriteResponse,
    AskJobCompleteActionsRequest, AskJobCompleteActionsResponse, CalendarDeleteResponse,
    CalendarExportQuery, CalendarExportResponse, CalendarImportRequest, CalendarImportResponse,
    CalendarListQuery, CalendarListResponse, CalendarWriteRequest, CalendarWriteResponse,
    CancelActiveSessionTurnResponse, CapabilityListResponse, CapabilityResolveResponse,
    ComponentRuntimeEventsRequest, ComponentRuntimeEventsResponse,
    ComponentRuntimeEventsTailResponse, ComponentRuntimeProbeResult, ComponentStoreDeleteResponse,
    ComponentStoreGetResponse, ComponentStoreListResponse, ComponentStoreSetRequest,
    ComponentStoreSetResponse, CreatePromptStashRequest, DecideAgentModeProposalRequest,
    DeletePromptStashResponse, DeleteRecurringResponse, DeriveSessionRequest,
    DeriveSessionResponse, EnqueueAskRequest, EnqueuePromptRequest, EnqueueReportRequest,
    EnqueueResponse, EnvironmentPendingResponse, EnvironmentProposeResponse,
    EnvironmentSpecPutRequest, EnvironmentSpecResponse, EnvironmentStatusResponse,
    EnvironmentValidateRequest, EnvironmentValidateResponse, FeedLatestGoodQuery,
    FeedLatestGoodResponse, FeedListResponse, FeedReadRequest, FeedTailQuery, FeedTailResponse,
    HealthResponse, IngestRequest, IngestResponse, InteractiveTurnRequest, InteractiveTurnResponse,
    JobReportResponse, JobResultResponse, LocalCatalogResponse, LocalEngineStatus,
    LocalHardwareResponse, LocalModelDownloadRequest, LocalModelDownloadResponse,
    LocalModelsResponse, McpGatewayStatusResponse, ModelDownloadProgress, PromptStash,
    PromptStashListResponse, RecurringDeliveryResponse, RecurringListQuery, RecurringListResponse,
    RecurringRunsQuery, RecurringRunsResponse, RegisterRecurringPromptRequest,
    RegisterRecurringResponse, RuntimeConfigCommandRequest, RuntimeConfigCommandResponse,
    SessionActiveTurnsResponse, SessionAgentModeResponse, SessionAppendTurnRequest,
    SessionAppendTurnResponse, SessionCodeBindingResponse, SessionCodeProjectResponse,
    SessionDeleteQuery, SessionDeleteResponse, SessionHistoryListResponse, SessionHistoryResponse,
    SessionSetDisplayNameRequest, SessionSetDisplayNameResponse, SessionTranscriptSearchResponse,
    SetSessionAgentModeRequest, SetSessionCodeBindingRequest, StageRouteCommandRequest,
    StageRouteCommandResponse, StartSessionCodeProjectRequest, TurnBudgetApproveRequest,
    TurnBudgetDenyRequest, TurnBudgetRequestListResponse, TurnBudgetRequestRecord,
    TurnBudgetRequestResponse, UpdateRecurringRequest, UpdateRecurringResponse,
    VaultAddRootRequest, VaultBacklinksQuery, VaultBacklinksResponse, VaultDeleteResponse,
    VaultNoteContentResponse, VaultNotesListResponse, VaultNotesQuery, VaultRootsResponse,
    VaultSearchQuery, VaultSearchResponse, VaultSetActiveRootRequest, VaultTagsListResponse,
    VaultTagsQuery, VaultWriteRequest, VaultWriteResponse, WorkCardDetail,
    WorkspaceCardActionResponse, WorkspaceCardsQuery, WorkspaceCardsResponse, WorkspaceFeedQuery,
    WorkspaceFeedResponse, WorkspaceLinkVaultRequest, WorkspaceSnapshot, WorkspaceSnapshotQuery,
};

#[cfg(feature = "blocking")]
use crate::SdkError;
#[cfg(feature = "blocking")]
use crate::generated::ops;
#[cfg(feature = "blocking")]
use crate::op::{op_path, op_path_query};
#[cfg(feature = "blocking")]
use crate::transport::path_with_query;

#[cfg(feature = "blocking")]
struct SyncHttp {
    client: reqwest::blocking::Client,
    base_url: String,
}

#[cfg(feature = "blocking")]
impl SyncHttp {
    fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }

    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, SdkError> {
        let value = self.request(reqwest::Method::GET, path, None)?;
        serde_json::from_value(value).map_err(Into::into)
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, SdkError> {
        let url = self.url(path);
        let mut builder = self.client.request(method, url);
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let response = builder.send().map_err(|e| SdkError::Http(e.to_string()))?;
        let status = response.status();
        let text = response.text().map_err(|e| SdkError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(SdkError::Http(format!("{status}: {text}")));
        }
        if text.trim().is_empty() {
            return Ok(serde_json::Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    fn post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, SdkError> {
        let body = serde_json::to_value(body).map_err(|e| SdkError::Serde(e.to_string()))?;
        let value = self.request(reqwest::Method::POST, path, Some(body))?;
        serde_json::from_value(value).map_err(Into::into)
    }

    fn post_with_header<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
        header: (&'static str, &str),
    ) -> Result<T, SdkError> {
        let response = self
            .client
            .post(self.url(path))
            .header(header.0, header.1)
            .json(body)
            .send()
            .map_err(|error| SdkError::Http(error.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|error| SdkError::Http(error.to_string()))?;
        if !status.is_success() {
            return Err(SdkError::Http(format!("{status}: {text}")));
        }
        serde_json::from_str(&text).map_err(Into::into)
    }

    fn post_empty<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, SdkError> {
        let value = self.request(reqwest::Method::POST, path, None)?;
        serde_json::from_value(value).map_err(Into::into)
    }

    fn put<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, SdkError> {
        let body = serde_json::to_value(body).map_err(|e| SdkError::Serde(e.to_string()))?;
        let value = self.request(reqwest::Method::PUT, path, Some(body))?;
        serde_json::from_value(value).map_err(Into::into)
    }

    fn patch<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, SdkError> {
        let body = serde_json::to_value(body).map_err(|e| SdkError::Serde(e.to_string()))?;
        let value = self.request(reqwest::Method::PATCH, path, Some(body))?;
        serde_json::from_value(value).map_err(Into::into)
    }

    fn delete<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, SdkError> {
        let value = self.request(reqwest::Method::DELETE, path, None)?;
        serde_json::from_value(value).map_err(Into::into)
    }
}

#[cfg(feature = "blocking")]
macro_rules! blocking_api {
    ($name:ident) => {
        pub struct $name<'a> {
            http: &'a SyncHttp,
        }
    };
}

#[cfg(feature = "blocking")]
blocking_api!(BlockingHealthApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingIngestApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingLocalModelsApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingJobsApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingRecurringApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingSessionsApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingPromptStashesApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingInteractiveApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingRuntimeApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingCapabilitiesApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingMcpGatewayApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingBudgetApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingVaultApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingCalendarApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingEnvironmentApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingComponentsApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingFeedsApi);
#[cfg(feature = "blocking")]
blocking_api!(BlockingWorkspaceApi);

#[cfg(feature = "blocking")]
pub struct BlockingMedousaClient {
    http: SyncHttp,
}

#[cfg(feature = "blocking")]
impl BlockingMedousaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: SyncHttp::new(base_url),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.http.base_url
    }

    pub fn health(&self) -> BlockingHealthApi<'_> {
        BlockingHealthApi { http: &self.http }
    }

    pub fn ingest(&self) -> BlockingIngestApi<'_> {
        BlockingIngestApi { http: &self.http }
    }

    pub fn local_models(&self) -> BlockingLocalModelsApi<'_> {
        BlockingLocalModelsApi { http: &self.http }
    }

    pub fn jobs(&self) -> BlockingJobsApi<'_> {
        BlockingJobsApi { http: &self.http }
    }

    pub fn recurring(&self) -> BlockingRecurringApi<'_> {
        BlockingRecurringApi { http: &self.http }
    }

    pub fn sessions(&self) -> BlockingSessionsApi<'_> {
        BlockingSessionsApi { http: &self.http }
    }

    pub fn prompt_stashes(&self) -> BlockingPromptStashesApi<'_> {
        BlockingPromptStashesApi { http: &self.http }
    }

    pub fn interactive(&self) -> BlockingInteractiveApi<'_> {
        BlockingInteractiveApi { http: &self.http }
    }

    pub fn runtime(&self) -> BlockingRuntimeApi<'_> {
        BlockingRuntimeApi { http: &self.http }
    }

    pub fn capabilities(&self) -> BlockingCapabilitiesApi<'_> {
        BlockingCapabilitiesApi { http: &self.http }
    }

    pub fn mcp_gateway(&self) -> BlockingMcpGatewayApi<'_> {
        BlockingMcpGatewayApi { http: &self.http }
    }

    pub fn budget(&self) -> BlockingBudgetApi<'_> {
        BlockingBudgetApi { http: &self.http }
    }

    pub fn vault(&self) -> BlockingVaultApi<'_> {
        BlockingVaultApi { http: &self.http }
    }

    pub fn calendar(&self) -> BlockingCalendarApi<'_> {
        BlockingCalendarApi { http: &self.http }
    }

    pub fn environment(&self) -> BlockingEnvironmentApi<'_> {
        BlockingEnvironmentApi { http: &self.http }
    }

    pub fn components(&self) -> BlockingComponentsApi<'_> {
        BlockingComponentsApi { http: &self.http }
    }

    pub fn feeds(&self) -> BlockingFeedsApi<'_> {
        BlockingFeedsApi { http: &self.http }
    }

    pub fn workspace(&self) -> BlockingWorkspaceApi<'_> {
        BlockingWorkspaceApi { http: &self.http }
    }
}

#[cfg(feature = "blocking")]
impl BlockingHealthApi<'_> {
    pub fn get(&self) -> Result<HealthResponse, SdkError> {
        let path = crate::generated::ops::HEALTH_GET.path;
        let value = self.http.request(reqwest::Method::GET, path, None)?;
        crate::health::decode_health(value, path)
    }
}

#[cfg(feature = "blocking")]
impl BlockingIngestApi<'_> {
    pub fn post(&self, request: &IngestRequest) -> Result<IngestResponse, SdkError> {
        self.http
            .post(crate::generated::ops::INGEST_POST.path, request)
    }
}

#[cfg(feature = "blocking")]
impl BlockingLocalModelsApi<'_> {
    pub fn hardware(&self) -> Result<LocalHardwareResponse, SdkError> {
        self.http.get(ops::LOCAL_HARDWARE_GET.path)
    }

    pub fn catalog(&self) -> Result<LocalCatalogResponse, SdkError> {
        self.http.get(ops::LOCAL_CATALOG_GET.path)
    }

    pub fn list(&self) -> Result<LocalModelsResponse, SdkError> {
        self.http.get(ops::LOCAL_MODELS_GET.path)
    }

    pub fn engine_status(&self) -> Result<LocalEngineStatus, SdkError> {
        self.http.get(ops::LOCAL_ENGINE_STATUS_GET.path)
    }

    pub fn start_download(&self, model_id: &str) -> Result<LocalModelDownloadResponse, SdkError> {
        self.http.post(
            ops::LOCAL_MODELS_DOWNLOAD_POST.path,
            &LocalModelDownloadRequest {
                model_id: model_id.to_string(),
            },
        )
    }

    pub fn download_status(&self, job_id: &str) -> Result<ModelDownloadProgress, SdkError> {
        self.http.get(&op_path(
            &ops::LOCAL_MODELS_DOWNLOAD_BY_JOB_ID_GET,
            &[("job_id", job_id.trim())],
        )?)
    }

    pub fn remove_model(&self, model_id: &str) -> Result<serde_json::Value, SdkError> {
        self.http.request(
            reqwest::Method::DELETE,
            &op_path(
                &ops::LOCAL_MODELS_BY_MODEL_ID_DELETE,
                &[("model_id", model_id)],
            )?,
            None,
        )
    }
}

#[cfg(feature = "blocking")]
impl BlockingJobsApi<'_> {
    pub fn enqueue_ask(&self, request: &EnqueueAskRequest) -> Result<EnqueueResponse, SdkError> {
        self.http.post(ops::JOBS_ASK_POST.path, request)
    }

    pub fn result(&self, job_id: &str) -> Result<JobResultResponse, SdkError> {
        self.http.get(&op_path(
            &ops::JOBS_BY_JOB_ID_RESULT_GET,
            &[("job_id", job_id.trim())],
        )?)
    }

    pub fn report(&self, job_id: &str) -> Result<JobReportResponse, SdkError> {
        self.http.get(&op_path(
            &ops::JOBS_BY_JOB_ID_REPORT_GET,
            &[("job_id", job_id.trim())],
        )?)
    }

    pub fn enqueue_report(
        &self,
        request: &EnqueueReportRequest,
    ) -> Result<EnqueueResponse, SdkError> {
        self.http.post(ops::JOBS_REPORT_POST.path, request)
    }

    pub fn enqueue_prompt(
        &self,
        request: &EnqueuePromptRequest,
    ) -> Result<EnqueueResponse, SdkError> {
        self.http.post(ops::JOBS_PROMPT_POST.path, request)
    }

    pub fn complete_actions(
        &self,
        job_id: &str,
        request: &AskJobCompleteActionsRequest,
    ) -> Result<AskJobCompleteActionsResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::JOBS_BY_JOB_ID_COMPLETE_ACTIONS_POST,
                &[("job_id", job_id.trim())],
            )?,
            request,
        )
    }

    pub fn archive(
        &self,
        job_id: &str,
        request: &ArchiveAskJobRequest,
    ) -> Result<ArchiveAskJobResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::JOBS_BY_JOB_ID_ARCHIVE_POST,
                &[("job_id", job_id.trim())],
            )?,
            request,
        )
    }
}

#[cfg(feature = "blocking")]
impl BlockingRecurringApi<'_> {
    pub fn register_prompt(
        &self,
        request: &RegisterRecurringPromptRequest,
    ) -> Result<RegisterRecurringResponse, SdkError> {
        self.http.post(ops::RECURRING_PROMPT_POST.path, request)
    }

    pub fn list(&self, query: &RecurringListQuery) -> Result<RecurringListResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(enabled_only) = query.enabled_only {
            params.push(("enabled_only", enabled_only.to_string()));
        }
        let path = op_path_query(&ops::RECURRING_GET, &[], &params)?;
        self.http.get(&path)
    }

    pub fn update(
        &self,
        recurring_id: &str,
        request: &UpdateRecurringRequest,
    ) -> Result<UpdateRecurringResponse, SdkError> {
        self.http.patch(
            &op_path(
                &ops::RECURRING_BY_RECURRING_ID_PATCH,
                &[("recurring_id", recurring_id.trim())],
            )?,
            request,
        )
    }

    pub fn delete(&self, recurring_id: &str) -> Result<DeleteRecurringResponse, SdkError> {
        self.http.delete(&op_path(
            &ops::RECURRING_BY_RECURRING_ID_DELETE,
            &[("recurring_id", recurring_id.trim())],
        )?)
    }

    pub fn runs(
        &self,
        recurring_id: &str,
        query: &RecurringRunsQuery,
    ) -> Result<RecurringRunsResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        let path = path_with_query(
            &op_path(
                &ops::RECURRING_BY_RECURRING_ID_RUNS_GET,
                &[("recurring_id", recurring_id.trim())],
            )?,
            &params,
        );
        self.http.get(&path)
    }

    pub fn delivery_status(
        &self,
        recurring_id: &str,
    ) -> Result<RecurringDeliveryResponse, SdkError> {
        self.http.get(&op_path(
            &ops::RECURRING_BY_RECURRING_ID_DELIVERY_GET,
            &[("recurring_id", recurring_id.trim())],
        )?)
    }
}

#[cfg(feature = "blocking")]
impl BlockingSessionsApi<'_> {
    pub fn list(&self, limit: usize) -> Result<SessionHistoryListResponse, SdkError> {
        self.http.get(&op_path_query(
            &ops::SESSIONS_GET,
            &[],
            &[("limit", limit.to_string())],
        )?)
    }

    pub fn search_transcripts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SessionTranscriptSearchResponse, SdkError> {
        self.http.get(&op_path_query(
            &ops::SESSIONS_SEARCH_GET,
            &[],
            &[("q", query.to_string()), ("limit", limit.to_string())],
        )?)
    }

    pub fn history(&self, session_id: &str) -> Result<SessionHistoryResponse, SdkError> {
        self.http.get(&op_path(
            &ops::SESSIONS_BY_SESSION_ID_HISTORY_GET,
            &[("session_id", session_id)],
        )?)
    }

    pub fn history_page(
        &self,
        session_id: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SessionHistoryResponse, SdkError> {
        let mut query = vec![("limit", limit.max(1).to_string())];
        if let Some(cursor) = cursor.map(str::trim).filter(|cursor| !cursor.is_empty()) {
            query.push(("cursor", cursor.to_string()));
        }
        self.http.get(&op_path_query(
            &ops::SESSIONS_BY_SESSION_ID_HISTORY_GET,
            &[("session_id", session_id)],
            &query,
        )?)
    }

    pub fn derive(
        &self,
        request: &DeriveSessionRequest,
        idempotency_key: &str,
    ) -> Result<DeriveSessionResponse, SdkError> {
        self.http.post_with_header(
            ops::SESSIONS_DERIVE_POST.path,
            request,
            ("Idempotency-Key", idempotency_key),
        )
    }

    pub fn set_display_name(
        &self,
        session_id: &str,
        display_name: &str,
    ) -> Result<SessionSetDisplayNameResponse, SdkError> {
        self.http.put(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_NAME_PUT,
                &[("session_id", session_id)],
            )?,
            &SessionSetDisplayNameRequest {
                display_name: display_name.to_string(),
            },
        )
    }

    pub fn agent_mode(&self, session_id: &str) -> Result<SessionAgentModeResponse, SdkError> {
        self.http.get(&op_path(
            &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_GET,
            &[("session_id", session_id)],
        )?)
    }

    pub fn set_agent_mode(
        &self,
        session_id: &str,
        request: &SetSessionAgentModeRequest,
    ) -> Result<SessionAgentModeResponse, SdkError> {
        self.http.put(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_PUT,
                &[("session_id", session_id)],
            )?,
            request,
        )
    }

    pub fn clear_agent_mode(
        &self,
        session_id: &str,
        scope: AgentModeScope,
    ) -> Result<SessionAgentModeResponse, SdkError> {
        let scope = match scope {
            AgentModeScope::Session => "session",
            AgentModeScope::Task => "task",
        };
        self.http.delete(&path_with_query(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_DELETE,
                &[("session_id", session_id)],
            )?,
            &[("scope", scope.to_string())],
        ))
    }

    pub fn agent_mode_proposals(
        &self,
        session_id: &str,
    ) -> Result<AgentModeProposalListResponse, SdkError> {
        self.http.get(&op_path(
            &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_PROPOSALS_GET,
            &[("session_id", session_id)],
        )?)
    }

    pub fn decide_agent_mode_proposal(
        &self,
        session_id: &str,
        proposal_id: &str,
        accept: bool,
    ) -> Result<AgentModeProposalResponse, SdkError> {
        self.http.put(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_PROPOSALS_BY_PROPOSAL_ID_PUT,
                &[("session_id", session_id), ("proposal_id", proposal_id)],
            )?,
            &DecideAgentModeProposalRequest { accept },
        )
    }

    pub fn code_binding(&self, session_id: &str) -> Result<SessionCodeBindingResponse, SdkError> {
        self.http.get(&op_path(
            &ops::SESSIONS_BY_SESSION_ID_CODE_BINDING_GET,
            &[("session_id", session_id)],
        )?)
    }

    pub fn set_code_binding(
        &self,
        session_id: &str,
        work_id: &str,
    ) -> Result<SessionCodeBindingResponse, SdkError> {
        self.http.put(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_CODE_BINDING_PUT,
                &[("session_id", session_id)],
            )?,
            &SetSessionCodeBindingRequest {
                work_id: work_id.to_string(),
            },
        )
    }

    pub fn clear_code_binding(
        &self,
        session_id: &str,
    ) -> Result<SessionCodeBindingResponse, SdkError> {
        self.http.delete(&op_path(
            &ops::SESSIONS_BY_SESSION_ID_CODE_BINDING_DELETE,
            &[("session_id", session_id)],
        )?)
    }

    pub fn start_code_project(
        &self,
        session_id: &str,
        request: &StartSessionCodeProjectRequest,
    ) -> Result<SessionCodeProjectResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_CODE_PROJECT_POST,
                &[("session_id", session_id)],
            )?,
            request,
        )
    }

    pub fn append_turn(
        &self,
        session_id: &str,
        request: &SessionAppendTurnRequest,
    ) -> Result<SessionAppendTurnResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_TURNS_POST,
                &[("session_id", session_id)],
            )?,
            request,
        )
    }

    pub fn delete(
        &self,
        session_id: &str,
        query: &SessionDeleteQuery,
    ) -> Result<SessionDeleteResponse, SdkError> {
        let path = path_with_query(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_DELETE,
                &[("session_id", session_id)],
            )?,
            &[("purge_memory", query.purge_memory.to_string())],
        );
        self.http.delete(&path)
    }

    pub fn list_turns(&self, session_id: &str) -> Result<SessionActiveTurnsResponse, SdkError> {
        self.http.get(&op_path(
            &ops::SESSIONS_BY_SESSION_ID_TURNS_GET,
            &[("session_id", session_id)],
        )?)
    }

    pub fn active_turn(&self, session_id: &str) -> Result<ActiveSessionTurnResponse, SdkError> {
        self.http.get(&op_path(
            &ops::SESSIONS_BY_SESSION_ID_ACTIVE_TURN_GET,
            &[("session_id", session_id)],
        )?)
    }

    pub fn cancel_active_turn(
        &self,
        session_id: &str,
    ) -> Result<CancelActiveSessionTurnResponse, SdkError> {
        self.http.post_empty(&op_path(
            &ops::SESSIONS_BY_SESSION_ID_ACTIVE_TURN_POST,
            &[("session_id", session_id)],
        )?)
    }
}

#[cfg(feature = "blocking")]
impl BlockingPromptStashesApi<'_> {
    pub fn list(&self) -> Result<PromptStashListResponse, SdkError> {
        self.http.get(ops::PROMPT_STASHES_GET.path)
    }

    pub fn create(&self, request: &CreatePromptStashRequest) -> Result<PromptStash, SdkError> {
        self.http.post(ops::PROMPT_STASHES_POST.path, request)
    }

    pub fn delete(&self, stash_id: &str) -> Result<DeletePromptStashResponse, SdkError> {
        self.http.delete(&op_path(
            &ops::PROMPT_STASHES_BY_STASH_ID_DELETE,
            &[("stash_id", stash_id)],
        )?)
    }
}

#[cfg(feature = "blocking")]
impl BlockingInteractiveApi<'_> {
    pub fn start_turn(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<InteractiveTurnResponse, SdkError> {
        self.http.post(ops::INTERACTIVE_TURN_POST.path, request)
    }

    pub fn cancel(&self, session_id: &str) -> Result<serde_json::Value, SdkError> {
        self.http.request(
            reqwest::Method::POST,
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_ACTIVE_TURN_POST,
                &[("session_id", session_id)],
            )?,
            None,
        )
    }
}

#[cfg(feature = "blocking")]
impl BlockingRuntimeApi<'_> {
    pub fn agent_modes(&self) -> Result<AgentModeListResponse, SdkError> {
        self.http.get(ops::AGENT_MODES_GET.path)
    }

    pub fn agent_mode_transition_policy(&self) -> Result<AgentModeTransitionPolicy, SdkError> {
        self.http.get(ops::AGENT_MODES_POLICY_GET.path)
    }

    pub fn set_agent_mode_transition_policy(
        &self,
        policy: &AgentModeTransitionPolicy,
    ) -> Result<AgentModeTransitionPolicy, SdkError> {
        self.http.put(ops::AGENT_MODES_POLICY_PUT.path, policy)
    }

    pub fn artifact_command(
        &self,
        request: &ArtifactCommandRequest,
    ) -> Result<ArtifactCommandResponse, SdkError> {
        self.http
            .post(ops::RUNTIME_ARTIFACT_COMMAND_POST.path, request)
    }

    pub fn artifact_fetch(
        &self,
        request: &ArtifactFetchRequest,
    ) -> Result<ArtifactFetchResponse, SdkError> {
        self.http
            .post(ops::RUNTIME_ARTIFACT_FETCH_POST.path, request)
    }

    pub fn artifact_list_ui(
        &self,
        request: &ArtifactListUiRequest,
    ) -> Result<ArtifactListUiResponse, SdkError> {
        self.http
            .post(ops::RUNTIME_ARTIFACT_LIST_UI_POST.path, request)
    }

    pub fn artifact_write(
        &self,
        request: &ArtifactWriteRequest,
    ) -> Result<ArtifactWriteResponse, SdkError> {
        self.http
            .post(ops::RUNTIME_ARTIFACT_WRITE_POST.path, request)
    }

    pub fn artifact_delete(
        &self,
        request: &ArtifactDeleteRequest,
    ) -> Result<ArtifactDeleteResponse, SdkError> {
        self.http
            .post(ops::RUNTIME_ARTIFACT_DELETE_POST.path, request)
    }

    pub fn config_command(
        &self,
        request: &RuntimeConfigCommandRequest,
    ) -> Result<RuntimeConfigCommandResponse, SdkError> {
        self.http
            .post(ops::RUNTIME_CONFIG_COMMAND_POST.path, request)
    }

    pub fn stage_route_command(
        &self,
        request: &StageRouteCommandRequest,
    ) -> Result<StageRouteCommandResponse, SdkError> {
        self.http
            .post(ops::RUNTIME_STAGE_ROUTE_COMMAND_POST.path, request)
    }
}

#[cfg(feature = "blocking")]
impl BlockingCapabilitiesApi<'_> {
    pub fn list(&self) -> Result<CapabilityListResponse, SdkError> {
        self.http.get(ops::CAPABILITIES_GET.path)
    }

    pub fn get(&self, capability_id: &str) -> Result<CapabilityResolveResponse, SdkError> {
        let path = op_path(
            &ops::CAPABILITIES_BY_CAPABILITY_ID_GET,
            &[("capability_id", capability_id.trim())],
        )?;
        self.http.get(&path)
    }

    pub fn reindex(&self) -> Result<serde_json::Value, SdkError> {
        self.http.request(
            reqwest::Method::POST,
            ops::CAPABILITIES_REINDEX_POST.path,
            None,
        )
    }
}

#[cfg(feature = "blocking")]
impl BlockingMcpGatewayApi<'_> {
    pub fn status(&self) -> Result<McpGatewayStatusResponse, SdkError> {
        self.http.get(ops::MCP_GATEWAY_STATUS_GET.path)
    }
}

#[cfg(feature = "blocking")]
impl BlockingBudgetApi<'_> {
    pub fn list(&self, pending_only: bool) -> Result<TurnBudgetRequestListResponse, SdkError> {
        let path = if pending_only {
            op_path_query(
                &ops::TURNS_BUDGET_REQUESTS_GET,
                &[],
                &[
                    ("status", "pending".to_string()),
                    ("limit", "20".to_string()),
                ],
            )?
        } else {
            op_path_query(
                &ops::TURNS_BUDGET_REQUESTS_GET,
                &[],
                &[("limit", "20".to_string())],
            )?
        };
        self.http.get(&path)
    }

    pub fn get(&self, request_id: &str) -> Result<TurnBudgetRequestRecord, SdkError> {
        self.http.get(&op_path(
            &ops::TURNS_BUDGET_REQUESTS_BY_REQUEST_ID_GET,
            &[("request_id", request_id.trim())],
        )?)
    }

    pub fn approve(
        &self,
        request_id: &str,
        body: &TurnBudgetApproveRequest,
    ) -> Result<TurnBudgetRequestResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::TURNS_BUDGET_REQUESTS_BY_REQUEST_ID_APPROVE_POST,
                &[("request_id", request_id.trim())],
            )?,
            body,
        )
    }

    pub fn deny(
        &self,
        request_id: &str,
        body: &TurnBudgetDenyRequest,
    ) -> Result<TurnBudgetRequestResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::TURNS_BUDGET_REQUESTS_BY_REQUEST_ID_DENY_POST,
                &[("request_id", request_id.trim())],
            )?,
            body,
        )
    }
}

#[cfg(feature = "blocking")]
impl BlockingVaultApi<'_> {
    pub fn list_roots(&self) -> Result<VaultRootsResponse, SdkError> {
        self.http.get(ops::VAULT_ROOTS_GET.path)
    }

    pub fn add_root(&self, request: &VaultAddRootRequest) -> Result<VaultRootsResponse, SdkError> {
        self.http.post(ops::VAULT_ROOTS_POST.path, request)
    }

    pub fn set_active_root(
        &self,
        request: &VaultSetActiveRootRequest,
    ) -> Result<VaultRootsResponse, SdkError> {
        self.http.put(ops::VAULT_ACTIVE_PUT.path, request)
    }

    pub fn list_notes(&self, query: &VaultNotesQuery) -> Result<VaultNotesListResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(prefix) = &query.prefix {
            params.push(("prefix", prefix.clone()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(tags) = &query.tags {
            params.push(("tags", tags.clone()));
        }
        if let Some(tag_prefix) = &query.tag_prefix {
            params.push(("tag_prefix", tag_prefix.clone()));
        }
        self.http
            .get(&op_path_query(&ops::VAULT_NOTES_GET, &[], &params)?)
    }

    pub fn create_note(&self, request: &VaultWriteRequest) -> Result<VaultWriteResponse, SdkError> {
        self.http.post(ops::VAULT_NOTES_POST.path, request)
    }

    pub fn get_note(&self, note_path: &str) -> Result<VaultNoteContentResponse, SdkError> {
        self.http.get(&op_path(
            &ops::VAULT_NOTES_BY_NOTE_PATH_GET,
            &[("note_path", note_path.trim_start_matches('/'))],
        )?)
    }

    pub fn delete_note(&self, note_path: &str) -> Result<VaultDeleteResponse, SdkError> {
        self.http.delete(&op_path(
            &ops::VAULT_NOTES_BY_NOTE_PATH_DELETE,
            &[("note_path", note_path.trim_start_matches('/'))],
        )?)
    }

    pub fn list_tags(&self, query: &VaultTagsQuery) -> Result<VaultTagsListResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(prefix) = &query.prefix {
            params.push(("prefix", prefix.clone()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        self.http
            .get(&op_path_query(&ops::VAULT_TAGS_GET, &[], &params)?)
    }

    pub fn search(&self, query: &VaultSearchQuery) -> Result<VaultSearchResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(q) = &query.q {
            params.push(("q", q.clone()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(tags) = &query.tags {
            params.push(("tags", tags.clone()));
        }
        self.http
            .get(&op_path_query(&ops::VAULT_SEARCH_GET, &[], &params)?)
    }

    pub fn backlinks(
        &self,
        query: &VaultBacklinksQuery,
    ) -> Result<VaultBacklinksResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        self.http
            .get(&op_path_query(&ops::VAULT_BACKLINKS_GET, &[], &params)?)
    }
}

#[cfg(feature = "blocking")]
impl BlockingCalendarApi<'_> {
    pub fn list_events(&self, query: &CalendarListQuery) -> Result<CalendarListResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(from) = query.from {
            params.push(("from", from.to_rfc3339()));
        }
        if let Some(to) = query.to {
            params.push(("to", to.to_rfc3339()));
        }
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        self.http
            .get(&op_path_query(&ops::CALENDAR_EVENTS_GET, &[], &params)?)
    }

    pub fn create_event(
        &self,
        request: &CalendarWriteRequest,
    ) -> Result<CalendarWriteResponse, SdkError> {
        self.http.post(ops::CALENDAR_EVENTS_POST.path, request)
    }

    pub fn update_event(
        &self,
        uid: &str,
        request: &CalendarWriteRequest,
    ) -> Result<CalendarWriteResponse, SdkError> {
        self.http.put(
            &op_path(&ops::CALENDAR_EVENTS_BY_UID_PUT, &[("uid", uid.trim())])?,
            request,
        )
    }

    pub fn delete_event(
        &self,
        uid: &str,
        query: &CalendarExportQuery,
    ) -> Result<CalendarDeleteResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        self.http.delete(&path_with_query(
            &op_path(&ops::CALENDAR_EVENTS_BY_UID_DELETE, &[("uid", uid.trim())])?,
            &params,
        ))
    }

    pub fn import_ics(
        &self,
        request: &CalendarImportRequest,
    ) -> Result<CalendarImportResponse, SdkError> {
        self.http.post(ops::CALENDAR_IMPORT_POST.path, request)
    }

    pub fn export(&self, query: &CalendarExportQuery) -> Result<CalendarExportResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        self.http
            .get(&op_path_query(&ops::CALENDAR_EXPORT_GET, &[], &params)?)
    }
}

#[cfg(feature = "blocking")]
impl BlockingWorkspaceApi<'_> {
    pub fn list_cards(
        &self,
        query: &WorkspaceCardsQuery,
    ) -> Result<WorkspaceCardsResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(session_id) = &query.session_id {
            params.push(("session_id", session_id.clone()));
        }
        if let Some(column) = &query.column {
            params.push(("column", column.clone()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(include_terminal) = query.include_terminal {
            params.push(("include_terminal", include_terminal.to_string()));
        }
        self.http
            .get(&op_path_query(&ops::WORKSPACE_CARDS_GET, &[], &params)?)
    }

    pub fn get_card(&self, card_id: &str) -> Result<WorkCardDetail, SdkError> {
        self.http.get(&op_path(
            &ops::WORKSPACE_CARDS_BY_CARD_ID_GET,
            &[("card_id", card_id.trim())],
        )?)
    }

    pub fn cancel_card(&self, card_id: &str) -> Result<WorkspaceCardActionResponse, SdkError> {
        self.http.post_empty(&op_path(
            &ops::WORKSPACE_CARDS_BY_CARD_ID_CANCEL_POST,
            &[("card_id", card_id.trim())],
        )?)
    }

    pub fn archive_card(
        &self,
        card_id: &str,
        request: &ArchiveAskJobRequest,
    ) -> Result<WorkspaceCardActionResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::WORKSPACE_CARDS_BY_CARD_ID_ARCHIVE_POST,
                &[("card_id", card_id.trim())],
            )?,
            request,
        )
    }

    pub fn retry_card(&self, card_id: &str) -> Result<WorkspaceCardActionResponse, SdkError> {
        self.http.post_empty(&op_path(
            &ops::WORKSPACE_CARDS_BY_CARD_ID_RETRY_POST,
            &[("card_id", card_id.trim())],
        )?)
    }

    pub fn link_vault(
        &self,
        card_id: &str,
        request: &WorkspaceLinkVaultRequest,
    ) -> Result<WorkspaceCardActionResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::WORKSPACE_CARDS_BY_CARD_ID_LINK_VAULT_POST,
                &[("card_id", card_id.trim())],
            )?,
            request,
        )
    }

    pub fn feed(&self, query: &WorkspaceFeedQuery) -> Result<WorkspaceFeedResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(since_id) = &query.since_id {
            params.push(("since_id", since_id.clone()));
        }
        if let Some(since_revision) = query.since_revision {
            params.push(("since_revision", since_revision.to_string()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(card_id) = &query.card_id {
            params.push(("card_id", card_id.clone()));
        }
        self.http
            .get(&op_path_query(&ops::WORKSPACE_FEED_GET, &[], &params)?)
    }

    pub fn snapshot(&self, query: &WorkspaceSnapshotQuery) -> Result<WorkspaceSnapshot, SdkError> {
        let mut params = Vec::new();
        if let Some(since_revision) = query.since_revision {
            params.push(("since_revision", since_revision.to_string()));
        }
        if let Some(feed_tail_limit) = query.feed_tail_limit {
            params.push(("feed_tail_limit", feed_tail_limit.to_string()));
        }
        self.http
            .get(&op_path_query(&ops::WORKSPACE_SNAPSHOT_GET, &[], &params)?)
    }
}

#[cfg(feature = "blocking")]
impl BlockingEnvironmentApi<'_> {
    fn profile_query(profile_id: Option<&str>) -> Vec<(&'static str, String)> {
        profile_id
            .map(|value| vec![("profile_id", value.to_string())])
            .unwrap_or_default()
    }

    pub fn get_spec(&self, profile_id: Option<&str>) -> Result<EnvironmentSpecResponse, SdkError> {
        self.http.get(&op_path_query(
            &ops::ENVIRONMENT_SPEC_GET,
            &[],
            &Self::profile_query(profile_id),
        )?)
    }

    pub fn put_spec(
        &self,
        request: &EnvironmentSpecPutRequest,
    ) -> Result<EnvironmentSpecResponse, SdkError> {
        self.http.put(ops::ENVIRONMENT_SPEC_PUT.path, request)
    }

    pub fn get_status(
        &self,
        profile_id: Option<&str>,
        surface_id: Option<&str>,
        include_runtime: Option<bool>,
    ) -> Result<EnvironmentStatusResponse, SdkError> {
        let mut params = Self::profile_query(profile_id);
        if let Some(surface) = surface_id {
            params.push(("surface_id", surface.to_string()));
        }
        if let Some(include) = include_runtime {
            params.push(("include_runtime", include.to_string()));
        }
        self.http
            .get(&op_path_query(&ops::ENVIRONMENT_STATUS_GET, &[], &params)?)
    }

    pub fn validate_spec(
        &self,
        request: &EnvironmentValidateRequest,
    ) -> Result<EnvironmentValidateResponse, SdkError> {
        self.http
            .post(ops::ENVIRONMENT_SPEC_VALIDATE_POST.path, request)
    }

    pub fn propose_spec(
        &self,
        request: &EnvironmentSpecPutRequest,
    ) -> Result<EnvironmentProposeResponse, SdkError> {
        self.http
            .post(ops::ENVIRONMENT_SPEC_PROPOSE_POST.path, request)
    }

    pub fn get_pending(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentPendingResponse, SdkError> {
        self.http.get(&op_path_query(
            &ops::ENVIRONMENT_SPEC_PENDING_GET,
            &[],
            &Self::profile_query(profile_id),
        )?)
    }

    pub fn dismiss_pending(&self, profile_id: Option<&str>) -> Result<(), SdkError> {
        self.http.delete::<serde_json::Value>(&op_path_query(
            &ops::ENVIRONMENT_SPEC_PENDING_DELETE,
            &[],
            &Self::profile_query(profile_id),
        )?)?;
        Ok(())
    }

    pub fn apply_pending(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentSpecResponse, SdkError> {
        self.http.post_empty(&op_path_query(
            &ops::ENVIRONMENT_SPEC_PENDING_APPLY_POST,
            &[],
            &Self::profile_query(profile_id),
        )?)
    }
}

#[cfg(feature = "blocking")]
impl BlockingComponentsApi<'_> {
    fn component_store_query(
        profile_id: Option<&str>,
        key: Option<&str>,
    ) -> Vec<(&'static str, String)> {
        let mut params = Vec::new();
        if let Some(profile) = profile_id {
            params.push(("profile_id", profile.to_string()));
        }
        if let Some(key) = key {
            params.push(("key", key.to_string()));
        }
        params
    }

    fn component_profile_query(profile_id: Option<&str>) -> Vec<(&'static str, String)> {
        profile_id
            .map(|value| vec![("profile_id", value.to_string())])
            .unwrap_or_default()
    }

    pub fn store_get(
        &self,
        component_id: &str,
        profile_id: Option<&str>,
        key: Option<&str>,
    ) -> Result<ComponentStoreGetResponse, SdkError> {
        self.http.get(&path_with_query(
            &op_path(
                &ops::COMPONENTS_BY_COMPONENT_ID_STORE_GET,
                &[("component_id", component_id.trim())],
            )?,
            &Self::component_store_query(profile_id, key),
        ))
    }

    pub fn store_set(
        &self,
        component_id: &str,
        key: &str,
        request: &ComponentStoreSetRequest,
    ) -> Result<ComponentStoreSetResponse, SdkError> {
        self.http.put(
            &path_with_query(
                &op_path(
                    &ops::COMPONENTS_BY_COMPONENT_ID_STORE_PUT,
                    &[("component_id", component_id.trim())],
                )?,
                &Self::component_store_query(None, Some(key)),
            ),
            request,
        )
    }

    pub fn store_list_keys(
        &self,
        component_id: &str,
        profile_id: Option<&str>,
    ) -> Result<ComponentStoreListResponse, SdkError> {
        self.http.get(&path_with_query(
            &op_path(
                &ops::COMPONENTS_BY_COMPONENT_ID_STORE_KEYS_GET,
                &[("component_id", component_id.trim())],
            )?,
            &Self::component_profile_query(profile_id),
        ))
    }

    pub fn store_get_key(
        &self,
        component_id: &str,
        key: &str,
        profile_id: Option<&str>,
    ) -> Result<ComponentStoreGetResponse, SdkError> {
        self.http.get(&path_with_query(
            &op_path(
                &ops::COMPONENTS_BY_COMPONENT_ID_STORE_BY_KEY_GET,
                &[("component_id", component_id.trim()), ("key", key.trim())],
            )?,
            &Self::component_profile_query(profile_id),
        ))
    }

    pub fn store_put_key(
        &self,
        component_id: &str,
        key: &str,
        request: &ComponentStoreSetRequest,
    ) -> Result<ComponentStoreSetResponse, SdkError> {
        self.http.put(
            &op_path(
                &ops::COMPONENTS_BY_COMPONENT_ID_STORE_BY_KEY_PUT,
                &[("component_id", component_id.trim()), ("key", key.trim())],
            )?,
            request,
        )
    }

    pub fn store_delete_key(
        &self,
        component_id: &str,
        key: &str,
        profile_id: Option<&str>,
    ) -> Result<ComponentStoreDeleteResponse, SdkError> {
        self.http.delete(&path_with_query(
            &op_path(
                &ops::COMPONENTS_BY_COMPONENT_ID_STORE_BY_KEY_DELETE,
                &[("component_id", component_id.trim()), ("key", key.trim())],
            )?,
            &Self::component_profile_query(profile_id),
        ))
    }

    pub fn runtime_tail_events(
        &self,
        component_id: &str,
        profile_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<ComponentRuntimeEventsTailResponse, SdkError> {
        let mut params = Self::component_profile_query(profile_id);
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        self.http.get(&path_with_query(
            &op_path(
                &ops::COMPONENTS_BY_COMPONENT_ID_RUNTIME_EVENTS_GET,
                &[("component_id", component_id.trim())],
            )?,
            &params,
        ))
    }

    pub fn runtime_append_events(
        &self,
        component_id: &str,
        request: &ComponentRuntimeEventsRequest,
    ) -> Result<ComponentRuntimeEventsResponse, SdkError> {
        self.http.post(
            &op_path(
                &ops::COMPONENTS_BY_COMPONENT_ID_RUNTIME_EVENTS_POST,
                &[("component_id", component_id.trim())],
            )?,
            request,
        )
    }

    pub fn runtime_complete_probe(
        &self,
        component_id: &str,
        probe_id: &str,
        request: &ComponentRuntimeProbeResult,
    ) -> Result<serde_json::Value, SdkError> {
        self.http.post(
            &op_path(
                &ops::COMPONENTS_BY_COMPONENT_ID_RUNTIME_PROBE_BY_PROBE_ID_RESULT_POST,
                &[
                    ("component_id", component_id.trim()),
                    ("probe_id", probe_id.trim()),
                ],
            )?,
            request,
        )
    }
}

#[cfg(feature = "blocking")]
impl BlockingFeedsApi<'_> {
    fn feed_tail_query_params(query: &FeedTailQuery) -> Vec<(&'static str, String)> {
        let mut params = Vec::new();
        if let Some(profile_id) = &query.profile_id {
            params.push(("profile_id", profile_id.clone()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        params
    }

    fn feed_profile_query(profile_id: Option<&str>) -> Vec<(&'static str, String)> {
        profile_id
            .map(|value| vec![("profile_id", value.to_string())])
            .unwrap_or_default()
    }

    pub fn list(&self, profile_id: Option<&str>) -> Result<FeedListResponse, SdkError> {
        self.http.get(&op_path_query(
            &ops::FEEDS_GET,
            &[],
            &Self::feed_profile_query(profile_id),
        )?)
    }

    pub fn tail(&self, feed_id: &str, query: &FeedTailQuery) -> Result<FeedTailResponse, SdkError> {
        self.http.get(&path_with_query(
            &op_path(
                &ops::FEEDS_BY_FEED_ID_TAIL_GET,
                &[("feed_id", feed_id.trim())],
            )?,
            &Self::feed_tail_query_params(query),
        ))
    }

    pub fn latest_good(
        &self,
        feed_id: &str,
        query: &FeedLatestGoodQuery,
    ) -> Result<FeedLatestGoodResponse, SdkError> {
        self.http.get(&path_with_query(
            &op_path(
                &ops::FEEDS_BY_FEED_ID_LATEST_GOOD_GET,
                &[("feed_id", feed_id.trim())],
            )?,
            &Self::feed_profile_query(query.profile_id.as_deref()),
        ))
    }

    pub fn mark_read(&self, feed_id: &str, request: &FeedReadRequest) -> Result<(), SdkError> {
        self.http.post::<serde_json::Value, _>(
            &op_path(
                &ops::FEEDS_BY_FEED_ID_READ_POST,
                &[("feed_id", feed_id.trim())],
            )?,
            request,
        )?;
        Ok(())
    }
}
