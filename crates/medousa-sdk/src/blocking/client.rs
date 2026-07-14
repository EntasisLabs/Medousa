#[cfg(feature = "blocking")]
use medousa_types::{
    ActiveSessionTurnResponse, ArchiveAskJobRequest, ArchiveAskJobResponse,
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactDeleteRequest, ArtifactDeleteResponse,
    ArtifactFetchRequest, ArtifactFetchResponse, ArtifactListUiRequest, ArtifactListUiResponse,
    ArtifactWriteRequest, ArtifactWriteResponse, AskJobCompleteActionsRequest,
    AskJobCompleteActionsResponse, CancelActiveSessionTurnResponse, CapabilityListResponse,
    CapabilityResolveResponse, DeleteRecurringResponse, EnqueueAskRequest, EnqueuePromptRequest,
    EnqueueReportRequest, EnqueueResponse, HealthResponse, IngestRequest, IngestResponse,
    InteractiveTurnRequest, InteractiveTurnResponse, JobReportResponse, JobResultResponse,
    LocalCatalogResponse, LocalEngineStatus, LocalHardwareResponse, LocalModelDownloadRequest,
    LocalModelDownloadResponse, LocalModelsResponse, McpGatewayStatusResponse,
    ModelDownloadProgress, RecurringDeliveryResponse, RecurringListQuery, RecurringListResponse,
    RecurringRunsQuery, RecurringRunsResponse, RegisterRecurringPromptRequest,
    RegisterRecurringResponse, RuntimeConfigCommandRequest, RuntimeConfigCommandResponse,
    SessionActiveTurnsResponse, SessionAppendTurnRequest, SessionAppendTurnResponse,
    SessionDeleteQuery, SessionDeleteResponse, SessionHistoryListResponse, SessionHistoryResponse,
    SessionSetDisplayNameRequest, SessionSetDisplayNameResponse, StageRouteCommandRequest,
    StageRouteCommandResponse, TurnBudgetApproveRequest, TurnBudgetDenyRequest,
    TurnBudgetRequestListResponse, TurnBudgetRequestRecord, TurnBudgetRequestResponse,
    UpdateRecurringRequest, UpdateRecurringResponse, VaultAddRootRequest, VaultBacklinksQuery,
    VaultBacklinksResponse, VaultDeleteResponse, VaultNoteContentResponse, VaultNotesListResponse,
    VaultNotesQuery, VaultRootsResponse, VaultSearchQuery, VaultSearchResponse,
    VaultSetActiveRootRequest, VaultTagsListResponse, VaultTagsQuery, VaultWriteRequest,
    VaultWriteResponse, WorkCardDetail, WorkspaceCardActionResponse, WorkspaceCardsQuery,
    WorkspaceCardsResponse, WorkspaceFeedQuery, WorkspaceFeedResponse, WorkspaceLinkVaultRequest,
    WorkspaceSnapshot, WorkspaceSnapshotQuery, ComponentRuntimeEventsRequest,
    ComponentRuntimeEventsResponse, ComponentRuntimeEventsTailResponse,
    ComponentRuntimeProbeResult, ComponentStoreDeleteResponse, ComponentStoreGetResponse,
    ComponentStoreListResponse, ComponentStoreSetRequest, ComponentStoreSetResponse,
    EnvironmentPendingResponse, EnvironmentProposeResponse, EnvironmentSpecPutRequest,
    EnvironmentSpecResponse, EnvironmentStatusResponse, EnvironmentValidateRequest,
    EnvironmentValidateResponse, FeedListResponse, FeedReadRequest, FeedTailQuery,
    FeedTailResponse, CalendarDeleteResponse, CalendarExportQuery, CalendarExportResponse,
    CalendarImportRequest, CalendarImportResponse, CalendarListQuery, CalendarListResponse,
    CalendarWriteRequest, CalendarWriteResponse,
};

#[cfg(feature = "blocking")]
use crate::transport::path_with_query;
#[cfg(feature = "blocking")]
use crate::SdkError;

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
        let text = response
            .text()
            .map_err(|e| SdkError::Http(e.to_string()))?;
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
        self.http.get("/health")
    }
}

#[cfg(feature = "blocking")]
impl BlockingIngestApi<'_> {
    pub fn post(&self, request: &IngestRequest) -> Result<IngestResponse, SdkError> {
        self.http.post("/v1/ingest", request)
    }
}

#[cfg(feature = "blocking")]
impl BlockingLocalModelsApi<'_> {
    pub fn hardware(&self) -> Result<LocalHardwareResponse, SdkError> {
        self.http.get("/v1/local/hardware")
    }

    pub fn catalog(&self) -> Result<LocalCatalogResponse, SdkError> {
        self.http.get("/v1/local/catalog")
    }

    pub fn list(&self) -> Result<LocalModelsResponse, SdkError> {
        self.http.get("/v1/local/models")
    }

    pub fn engine_status(&self) -> Result<LocalEngineStatus, SdkError> {
        self.http.get("/v1/local/engine/status")
    }

    pub fn start_download(&self, model_id: &str) -> Result<LocalModelDownloadResponse, SdkError> {
        self.http.post(
            "/v1/local/models/download",
            &LocalModelDownloadRequest {
                model_id: model_id.to_string(),
            },
        )
    }

    pub fn download_status(&self, job_id: &str) -> Result<ModelDownloadProgress, SdkError> {
        self.http
            .get(&format!("/v1/local/models/download/{}", job_id.trim()))
    }

    pub fn remove_model(&self, model_id: &str) -> Result<serde_json::Value, SdkError> {
        self.http
            .request(reqwest::Method::DELETE, &format!("/v1/local/models/{model_id}"), None)
    }
}

#[cfg(feature = "blocking")]
impl BlockingJobsApi<'_> {
    pub fn enqueue_ask(&self, request: &EnqueueAskRequest) -> Result<EnqueueResponse, SdkError> {
        self.http.post("/v1/jobs/ask", request)
    }

    pub fn result(&self, job_id: &str) -> Result<JobResultResponse, SdkError> {
        self.http
            .get(&format!("/v1/jobs/{}/result", job_id.trim()))
    }

    pub fn report(&self, job_id: &str) -> Result<JobReportResponse, SdkError> {
        self.http
            .get(&format!("/v1/jobs/{}/report", job_id.trim()))
    }

    pub fn enqueue_report(&self, request: &EnqueueReportRequest) -> Result<EnqueueResponse, SdkError> {
        self.http.post("/v1/jobs/report", request)
    }

    pub fn enqueue_prompt(&self, request: &EnqueuePromptRequest) -> Result<EnqueueResponse, SdkError> {
        self.http.post("/v1/jobs/prompt", request)
    }

    pub fn complete_actions(
        &self,
        job_id: &str,
        request: &AskJobCompleteActionsRequest,
    ) -> Result<AskJobCompleteActionsResponse, SdkError> {
        self.http
            .post(&format!("/v1/jobs/{}/complete-actions", job_id.trim()), request)
    }

    pub fn archive(
        &self,
        job_id: &str,
        request: &ArchiveAskJobRequest,
    ) -> Result<ArchiveAskJobResponse, SdkError> {
        self.http
            .post(&format!("/v1/jobs/{}/archive", job_id.trim()), request)
    }
}

#[cfg(feature = "blocking")]
impl BlockingRecurringApi<'_> {
    pub fn register_prompt(
        &self,
        request: &RegisterRecurringPromptRequest,
    ) -> Result<RegisterRecurringResponse, SdkError> {
        self.http.post("/v1/recurring/prompt", request)
    }

    pub fn list(&self, query: &RecurringListQuery) -> Result<RecurringListResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(enabled_only) = query.enabled_only {
            params.push(("enabled_only", enabled_only.to_string()));
        }
        let path = path_with_query("/v1/recurring", &params);
        self.http.get(&path)
    }

    pub fn update(
        &self,
        recurring_id: &str,
        request: &UpdateRecurringRequest,
    ) -> Result<UpdateRecurringResponse, SdkError> {
        self.http.patch(
            &format!("/v1/recurring/{}", recurring_id.trim()),
            request,
        )
    }

    pub fn delete(&self, recurring_id: &str) -> Result<DeleteRecurringResponse, SdkError> {
        self.http
            .delete(&format!("/v1/recurring/{}", recurring_id.trim()))
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
            &format!("/v1/recurring/{}/runs", recurring_id.trim()),
            &params,
        );
        self.http.get(&path)
    }

    pub fn delivery_status(
        &self,
        recurring_id: &str,
    ) -> Result<RecurringDeliveryResponse, SdkError> {
        self.http
            .get(&format!("/v1/recurring/{}/delivery", recurring_id.trim()))
    }
}

#[cfg(feature = "blocking")]
impl BlockingSessionsApi<'_> {
    pub fn list(&self, limit: usize) -> Result<SessionHistoryListResponse, SdkError> {
        self.http.get(&format!("/v1/sessions?limit={limit}"))
    }

    pub fn history(&self, session_id: &str) -> Result<SessionHistoryResponse, SdkError> {
        self.http
            .get(&format!("/v1/sessions/{session_id}/history"))
    }

    pub fn set_display_name(
        &self,
        session_id: &str,
        display_name: &str,
    ) -> Result<SessionSetDisplayNameResponse, SdkError> {
        self.http.put(
            &format!("/v1/sessions/{session_id}/name"),
            &SessionSetDisplayNameRequest {
                display_name: display_name.to_string(),
            },
        )
    }

    pub fn append_turn(
        &self,
        session_id: &str,
        request: &SessionAppendTurnRequest,
    ) -> Result<SessionAppendTurnResponse, SdkError> {
        self.http
            .post(&format!("/v1/sessions/{session_id}/turns"), request)
    }

    pub fn delete(
        &self,
        session_id: &str,
        query: &SessionDeleteQuery,
    ) -> Result<SessionDeleteResponse, SdkError> {
        let path = path_with_query(
            &format!("/v1/sessions/{session_id}"),
            &[("purge_memory", query.purge_memory.to_string())],
        );
        self.http.delete(&path)
    }

    pub fn list_turns(&self, session_id: &str) -> Result<SessionActiveTurnsResponse, SdkError> {
        self.http
            .get(&format!("/v1/sessions/{session_id}/turns"))
    }

    pub fn active_turn(&self, session_id: &str) -> Result<ActiveSessionTurnResponse, SdkError> {
        self.http
            .get(&format!("/v1/sessions/{session_id}/active-turn"))
    }

    pub fn cancel_active_turn(
        &self,
        session_id: &str,
    ) -> Result<CancelActiveSessionTurnResponse, SdkError> {
        self.http
            .post_empty(&format!("/v1/sessions/{session_id}/active-turn"))
    }
}

#[cfg(feature = "blocking")]
impl BlockingInteractiveApi<'_> {
    pub fn start_turn(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<InteractiveTurnResponse, SdkError> {
        self.http.post("/v1/interactive/turn", request)
    }

    pub fn cancel(&self, session_id: &str) -> Result<serde_json::Value, SdkError> {
        self.http
            .request(
                reqwest::Method::POST,
                &format!("/v1/sessions/{session_id}/active-turn"),
                None,
            )
    }
}

#[cfg(feature = "blocking")]
impl BlockingRuntimeApi<'_> {
    pub fn artifact_command(
        &self,
        request: &ArtifactCommandRequest,
    ) -> Result<ArtifactCommandResponse, SdkError> {
        self.http.post("/v1/runtime/artifact/command", request)
    }

    pub fn artifact_fetch(
        &self,
        request: &ArtifactFetchRequest,
    ) -> Result<ArtifactFetchResponse, SdkError> {
        self.http.post("/v1/runtime/artifact/fetch", request)
    }

    pub fn artifact_list_ui(
        &self,
        request: &ArtifactListUiRequest,
    ) -> Result<ArtifactListUiResponse, SdkError> {
        self.http.post("/v1/runtime/artifact/list-ui", request)
    }

    pub fn artifact_write(
        &self,
        request: &ArtifactWriteRequest,
    ) -> Result<ArtifactWriteResponse, SdkError> {
        self.http.post("/v1/runtime/artifact/write", request)
    }

    pub fn artifact_delete(
        &self,
        request: &ArtifactDeleteRequest,
    ) -> Result<ArtifactDeleteResponse, SdkError> {
        self.http.post("/v1/runtime/artifact/delete", request)
    }

    pub fn config_command(
        &self,
        request: &RuntimeConfigCommandRequest,
    ) -> Result<RuntimeConfigCommandResponse, SdkError> {
        self.http.post("/v1/runtime/config/command", request)
    }

    pub fn stage_route_command(
        &self,
        request: &StageRouteCommandRequest,
    ) -> Result<StageRouteCommandResponse, SdkError> {
        self.http.post("/v1/runtime/stage-route/command", request)
    }
}

#[cfg(feature = "blocking")]
impl BlockingCapabilitiesApi<'_> {
    pub fn list(&self) -> Result<CapabilityListResponse, SdkError> {
        self.http.get("/v1/capabilities")
    }

    pub fn get(&self, capability_id: &str) -> Result<CapabilityResolveResponse, SdkError> {
        let path = format!("/v1/capabilities/{}", urlencoding::encode(capability_id.trim()));
        self.http.get(&path)
    }

    pub fn reindex(&self) -> Result<serde_json::Value, SdkError> {
        self.http
            .request(reqwest::Method::POST, "/v1/capabilities/reindex", None)
    }
}

#[cfg(feature = "blocking")]
impl BlockingMcpGatewayApi<'_> {
    pub fn status(&self) -> Result<McpGatewayStatusResponse, SdkError> {
        self.http.get("/v1/mcp/gateway/status")
    }
}

#[cfg(feature = "blocking")]
impl BlockingBudgetApi<'_> {
    pub fn list(&self, pending_only: bool) -> Result<TurnBudgetRequestListResponse, SdkError> {
        let path = if pending_only {
            "/v1/turns/budget-requests?status=pending&limit=20"
        } else {
            "/v1/turns/budget-requests?limit=20"
        };
        self.http.get(path)
    }

    pub fn get(&self, request_id: &str) -> Result<TurnBudgetRequestRecord, SdkError> {
        self.http
            .get(&format!("/v1/turns/budget-requests/{}", request_id.trim()))
    }

    pub fn approve(
        &self,
        request_id: &str,
        body: &TurnBudgetApproveRequest,
    ) -> Result<TurnBudgetRequestResponse, SdkError> {
        self.http.post(
            &format!("/v1/turns/budget-requests/{}/approve", request_id.trim()),
            body,
        )
    }

    pub fn deny(
        &self,
        request_id: &str,
        body: &TurnBudgetDenyRequest,
    ) -> Result<TurnBudgetRequestResponse, SdkError> {
        self.http.post(
            &format!("/v1/turns/budget-requests/{}/deny", request_id.trim()),
            body,
        )
    }
}

#[cfg(feature = "blocking")]
impl BlockingVaultApi<'_> {
    pub fn list_roots(&self) -> Result<VaultRootsResponse, SdkError> {
        self.http.get("/v1/vault/roots")
    }

    pub fn add_root(&self, request: &VaultAddRootRequest) -> Result<VaultRootsResponse, SdkError> {
        self.http.post("/v1/vault/roots", request)
    }

    pub fn set_active_root(
        &self,
        request: &VaultSetActiveRootRequest,
    ) -> Result<VaultRootsResponse, SdkError> {
        self.http.put("/v1/vault/active", request)
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
        self.http.get(&path_with_query("/v1/vault/notes", &params))
    }

    pub fn create_note(&self, request: &VaultWriteRequest) -> Result<VaultWriteResponse, SdkError> {
        self.http.post("/v1/vault/notes", request)
    }

    pub fn get_note(&self, note_path: &str) -> Result<VaultNoteContentResponse, SdkError> {
        self.http
            .get(&format!("/v1/vault/notes/{}", note_path.trim_start_matches('/')))
    }

    pub fn delete_note(&self, note_path: &str) -> Result<VaultDeleteResponse, SdkError> {
        self.http
            .delete(&format!("/v1/vault/notes/{}", note_path.trim_start_matches('/')))
    }

    pub fn list_tags(&self, query: &VaultTagsQuery) -> Result<VaultTagsListResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(prefix) = &query.prefix {
            params.push(("prefix", prefix.clone()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        self.http.get(&path_with_query("/v1/vault/tags", &params))
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
        self.http.get(&path_with_query("/v1/vault/search", &params))
    }

    pub fn backlinks(
        &self,
        query: &VaultBacklinksQuery,
    ) -> Result<VaultBacklinksResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        self.http.get(&path_with_query("/v1/vault/backlinks", &params))
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
            .get(&path_with_query("/v1/calendar/events", &params))
    }

    pub fn create_event(
        &self,
        request: &CalendarWriteRequest,
    ) -> Result<CalendarWriteResponse, SdkError> {
        self.http.post("/v1/calendar/events", request)
    }

    pub fn update_event(
        &self,
        uid: &str,
        request: &CalendarWriteRequest,
    ) -> Result<CalendarWriteResponse, SdkError> {
        self.http
            .put(&format!("/v1/calendar/events/{}", uid.trim()), request)
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
            &format!("/v1/calendar/events/{}", uid.trim()),
            &params,
        ))
    }

    pub fn import_ics(
        &self,
        request: &CalendarImportRequest,
    ) -> Result<CalendarImportResponse, SdkError> {
        self.http.post("/v1/calendar/import", request)
    }

    pub fn export(&self, query: &CalendarExportQuery) -> Result<CalendarExportResponse, SdkError> {
        let mut params = Vec::new();
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        self.http
            .get(&path_with_query("/v1/calendar/export", &params))
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
            .get(&path_with_query("/v1/workspace/cards", &params))
    }

    pub fn get_card(&self, card_id: &str) -> Result<WorkCardDetail, SdkError> {
        self.http
            .get(&format!("/v1/workspace/cards/{}", card_id.trim()))
    }

    pub fn cancel_card(&self, card_id: &str) -> Result<WorkspaceCardActionResponse, SdkError> {
        self.http
            .post_empty(&format!("/v1/workspace/cards/{}/cancel", card_id.trim()))
    }

    pub fn archive_card(
        &self,
        card_id: &str,
        request: &ArchiveAskJobRequest,
    ) -> Result<WorkspaceCardActionResponse, SdkError> {
        self.http.post(
            &format!("/v1/workspace/cards/{}/archive", card_id.trim()),
            request,
        )
    }

    pub fn retry_card(&self, card_id: &str) -> Result<WorkspaceCardActionResponse, SdkError> {
        self.http
            .post_empty(&format!("/v1/workspace/cards/{}/retry", card_id.trim()))
    }

    pub fn link_vault(
        &self,
        card_id: &str,
        request: &WorkspaceLinkVaultRequest,
    ) -> Result<WorkspaceCardActionResponse, SdkError> {
        self.http.post(
            &format!("/v1/workspace/cards/{}/link-vault", card_id.trim()),
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
            .get(&path_with_query("/v1/workspace/feed", &params))
    }

    pub fn snapshot(
        &self,
        query: &WorkspaceSnapshotQuery,
    ) -> Result<WorkspaceSnapshot, SdkError> {
        let mut params = Vec::new();
        if let Some(since_revision) = query.since_revision {
            params.push(("since_revision", since_revision.to_string()));
        }
        if let Some(feed_tail_limit) = query.feed_tail_limit {
            params.push(("feed_tail_limit", feed_tail_limit.to_string()));
        }
        self.http
            .get(&path_with_query("/v1/workspace/snapshot", &params))
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
        self.http
            .get(&path_with_query("/v1/environment/spec", &Self::profile_query(profile_id)))
    }

    pub fn put_spec(
        &self,
        request: &EnvironmentSpecPutRequest,
    ) -> Result<EnvironmentSpecResponse, SdkError> {
        self.http.put("/v1/environment/spec", request)
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
            .get(&path_with_query("/v1/environment/status", &params))
    }

    pub fn validate_spec(
        &self,
        request: &EnvironmentValidateRequest,
    ) -> Result<EnvironmentValidateResponse, SdkError> {
        self.http.post("/v1/environment/spec/validate", request)
    }

    pub fn propose_spec(
        &self,
        request: &EnvironmentSpecPutRequest,
    ) -> Result<EnvironmentProposeResponse, SdkError> {
        self.http.post("/v1/environment/spec/propose", request)
    }

    pub fn get_pending(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentPendingResponse, SdkError> {
        self.http.get(&path_with_query(
            "/v1/environment/spec/pending",
            &Self::profile_query(profile_id),
        ))
    }

    pub fn dismiss_pending(&self, profile_id: Option<&str>) -> Result<(), SdkError> {
        self.http.delete::<serde_json::Value>(&path_with_query(
            "/v1/environment/spec/pending",
            &Self::profile_query(profile_id),
        ))?;
        Ok(())
    }

    pub fn apply_pending(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentSpecResponse, SdkError> {
        self.http.post_empty(&path_with_query(
            "/v1/environment/spec/pending/apply",
            &Self::profile_query(profile_id),
        ))
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
            &format!("/v1/components/{}/store", component_id.trim()),
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
                &format!("/v1/components/{}/store", component_id.trim()),
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
            &format!("/v1/components/{}/store/keys", component_id.trim()),
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
            &format!(
                "/v1/components/{}/store/{}",
                component_id.trim(),
                urlencoding::encode(key.trim())
            ),
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
            &format!(
                "/v1/components/{}/store/{}",
                component_id.trim(),
                urlencoding::encode(key.trim())
            ),
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
            &format!(
                "/v1/components/{}/store/{}",
                component_id.trim(),
                urlencoding::encode(key.trim())
            ),
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
            &format!("/v1/components/{}/runtime/events", component_id.trim()),
            &params,
        ))
    }

    pub fn runtime_append_events(
        &self,
        component_id: &str,
        request: &ComponentRuntimeEventsRequest,
    ) -> Result<ComponentRuntimeEventsResponse, SdkError> {
        self.http.post(
            &format!("/v1/components/{}/runtime/events", component_id.trim()),
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
            &format!(
                "/v1/components/{}/runtime/probe/{}/result",
                component_id.trim(),
                urlencoding::encode(probe_id.trim())
            ),
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
        self.http
            .get(&path_with_query("/v1/feeds", &Self::feed_profile_query(profile_id)))
    }

    pub fn tail(&self, feed_id: &str, query: &FeedTailQuery) -> Result<FeedTailResponse, SdkError> {
        self.http.get(&path_with_query(
            &format!("/v1/feeds/{}/tail", feed_id.trim()),
            &Self::feed_tail_query_params(query),
        ))
    }

    pub fn mark_read(&self, feed_id: &str, request: &FeedReadRequest) -> Result<(), SdkError> {
        self.http.post::<serde_json::Value, _>(
            &format!("/v1/feeds/{}/read", feed_id.trim()),
            request,
        )?;
        Ok(())
    }
}
