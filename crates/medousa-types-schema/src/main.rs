//! Export JSON Schema definitions for all public medousa-types structs.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use medousa_types::*;
use schemars::schema::RootSchema;
use schemars::schema_for;

macro_rules! export_type {
    ($map:expr, $ty:ty, $name:expr) => {
        $map.insert($name.to_string(), schema_for!($ty));
    };
}

fn main() {
    let mut schemas: BTreeMap<String, RootSchema> = BTreeMap::new();

    // Health & jobs
    export_type!(schemas, HealthResponse, "HealthResponse");
    export_type!(schemas, EnqueueAskRequest, "EnqueueAskRequest");
    export_type!(schemas, EnqueueReportRequest, "EnqueueReportRequest");
    export_type!(schemas, EnqueuePromptRequest, "EnqueuePromptRequest");
    export_type!(schemas, EnqueueResponse, "EnqueueResponse");
    export_type!(schemas, JobResultResponse, "JobResultResponse");
    export_type!(schemas, JobReportResponse, "JobReportResponse");
    export_type!(schemas, AskJobCompleteActionsRequest, "AskJobCompleteActionsRequest");
    export_type!(schemas, AskJobCompleteActionsResponse, "AskJobCompleteActionsResponse");
    export_type!(schemas, ArchiveAskJobRequest, "ArchiveAskJobRequest");
    export_type!(schemas, ArchiveAskJobResponse, "ArchiveAskJobResponse");

    // Sessions
    export_type!(schemas, SessionHistoryListResponse, "SessionHistoryListResponse");
    export_type!(schemas, SessionHistoryResponse, "SessionHistoryResponse");
    export_type!(schemas, SessionAppendTurnRequest, "SessionAppendTurnRequest");
    export_type!(schemas, SessionAppendTurnResponse, "SessionAppendTurnResponse");
    export_type!(schemas, SessionSetDisplayNameRequest, "SessionSetDisplayNameRequest");
    export_type!(schemas, SessionSetDisplayNameResponse, "SessionSetDisplayNameResponse");
    export_type!(schemas, SessionDeleteResponse, "SessionDeleteResponse");

    // Ingest & interactive
    export_type!(schemas, IngestRequest, "IngestRequest");
    export_type!(schemas, IngestResponse, "IngestResponse");
    export_type!(schemas, InteractiveTurnRequest, "InteractiveTurnRequest");
    export_type!(schemas, InteractiveTurnResponse, "InteractiveTurnResponse");
    export_type!(schemas, InteractiveTurnStreamEvent, "InteractiveTurnStreamEvent");

    // Recurring
    export_type!(schemas, RegisterRecurringPromptRequest, "RegisterRecurringPromptRequest");
    export_type!(schemas, RegisterRecurringResponse, "RegisterRecurringResponse");
    export_type!(schemas, RecurringListResponse, "RecurringListResponse");
    export_type!(schemas, UpdateRecurringRequest, "UpdateRecurringRequest");
    export_type!(schemas, UpdateRecurringResponse, "UpdateRecurringResponse");
    export_type!(schemas, DeleteRecurringResponse, "DeleteRecurringResponse");
    export_type!(schemas, RecurringRunsResponse, "RecurringRunsResponse");
    export_type!(schemas, RecurringDeliveryResponse, "RecurringDeliveryResponse");

    // Runtime artifacts
    export_type!(schemas, ArtifactCommandRequest, "ArtifactCommandRequest");
    export_type!(schemas, ArtifactCommandResponse, "ArtifactCommandResponse");
    export_type!(schemas, ArtifactFetchRequest, "ArtifactFetchRequest");
    export_type!(schemas, ArtifactFetchResponse, "ArtifactFetchResponse");
    export_type!(schemas, ArtifactWriteRequest, "ArtifactWriteRequest");
    export_type!(schemas, ArtifactWriteResponse, "ArtifactWriteResponse");
    export_type!(schemas, ArtifactDeleteRequest, "ArtifactDeleteRequest");
    export_type!(schemas, ArtifactDeleteResponse, "ArtifactDeleteResponse");
    export_type!(schemas, ArtifactListUiRequest, "ArtifactListUiRequest");
    export_type!(schemas, ArtifactListUiResponse, "ArtifactListUiResponse");
    export_type!(schemas, RuntimeConfigCommandRequest, "RuntimeConfigCommandRequest");
    export_type!(schemas, RuntimeConfigCommandResponse, "RuntimeConfigCommandResponse");
    export_type!(schemas, StageRouteCommandRequest, "StageRouteCommandRequest");
    export_type!(schemas, StageRouteCommandResponse, "StageRouteCommandResponse");

    // Budget
    export_type!(schemas, TurnBudgetApproveRequest, "TurnBudgetApproveRequest");
    export_type!(schemas, TurnBudgetDenyRequest, "TurnBudgetDenyRequest");
    export_type!(schemas, TurnBudgetRequestListResponse, "TurnBudgetRequestListResponse");
    export_type!(schemas, TurnBudgetRequestResponse, "TurnBudgetRequestResponse");

    // Local
    export_type!(schemas, LocalHardwareResponse, "LocalHardwareResponse");
    export_type!(schemas, LocalCatalogResponse, "LocalCatalogResponse");
    export_type!(schemas, LocalModelsResponse, "LocalModelsResponse");
    export_type!(schemas, LocalEngineStatus, "LocalEngineStatus");
    export_type!(schemas, LocalModelDownloadRequest, "LocalModelDownloadRequest");
    export_type!(schemas, LocalModelDownloadResponse, "LocalModelDownloadResponse");
    export_type!(schemas, ModelDownloadProgress, "ModelDownloadProgress");

    // Capabilities & MCP
    export_type!(schemas, CapabilityListResponse, "CapabilityListResponse");
    export_type!(schemas, CapabilityResolveResponse, "CapabilityResolveResponse");
    export_type!(schemas, McpGatewayStatusResponse, "McpGatewayStatusResponse");

    // Vault
    export_type!(schemas, VaultRootsResponse, "VaultRootsResponse");
    export_type!(schemas, VaultAddRootRequest, "VaultAddRootRequest");
    export_type!(schemas, VaultSetActiveRootRequest, "VaultSetActiveRootRequest");
    export_type!(schemas, VaultNotesListResponse, "VaultNotesListResponse");
    export_type!(schemas, VaultWriteRequest, "VaultWriteRequest");
    export_type!(schemas, VaultWriteResponse, "VaultWriteResponse");
    export_type!(schemas, VaultNoteContentResponse, "VaultNoteContentResponse");
    export_type!(schemas, VaultDeleteResponse, "VaultDeleteResponse");
    export_type!(schemas, VaultTagsListResponse, "VaultTagsListResponse");
    export_type!(schemas, VaultSearchResponse, "VaultSearchResponse");
    export_type!(schemas, VaultBacklinksResponse, "VaultBacklinksResponse");

    // Calendar
    export_type!(schemas, CalendarEvent, "CalendarEvent");
    export_type!(schemas, CalendarListResponse, "CalendarListResponse");
    export_type!(schemas, CalendarWriteRequest, "CalendarWriteRequest");
    export_type!(schemas, CalendarWriteResponse, "CalendarWriteResponse");
    export_type!(schemas, CalendarDeleteResponse, "CalendarDeleteResponse");
    export_type!(schemas, CalendarImportRequest, "CalendarImportRequest");
    export_type!(schemas, CalendarImportResponse, "CalendarImportResponse");
    export_type!(schemas, CalendarExportResponse, "CalendarExportResponse");

    // Agents (hot-swappable runtimes)
    export_type!(schemas, AgentRuntimeInfo, "AgentRuntimeInfo");
    export_type!(schemas, AgentRuntimeListResponse, "AgentRuntimeListResponse");
    export_type!(schemas, CreateAgentSessionRequest, "CreateAgentSessionRequest");
    export_type!(schemas, CreateAgentSessionResponse, "CreateAgentSessionResponse");
    export_type!(schemas, AgentSessionPromptRequest, "AgentSessionPromptRequest");
    export_type!(schemas, AgentSessionPromptResponse, "AgentSessionPromptResponse");
    export_type!(schemas, CancelAgentSessionResponse, "CancelAgentSessionResponse");
    export_type!(schemas, AgentPermissionRequestRecord, "AgentPermissionRequestRecord");
    export_type!(schemas, AgentPermissionRequestListResponse, "AgentPermissionRequestListResponse");
    export_type!(schemas, AgentPermissionResolveRequest, "AgentPermissionResolveRequest");
    export_type!(schemas, AgentPermissionResolveResponse, "AgentPermissionResolveResponse");

    // Workspace
    export_type!(schemas, WorkspaceCardActionResponse, "WorkspaceCardActionResponse");
    export_type!(schemas, WorkspaceLinkVaultRequest, "WorkspaceLinkVaultRequest");
    export_type!(schemas, WorkspaceStreamEvent, "WorkspaceStreamEvent");

    // Environment (canvas)
    export_type!(schemas, EnvironmentSpec, "EnvironmentSpec");
    export_type!(schemas, EnvironmentSpecResponse, "EnvironmentSpecResponse");
    export_type!(schemas, EnvironmentSpecPutRequest, "EnvironmentSpecPutRequest");
    export_type!(schemas, EnvironmentStatusResponse, "EnvironmentStatusResponse");
    export_type!(schemas, EnvironmentValidateRequest, "EnvironmentValidateRequest");
    export_type!(schemas, EnvironmentValidateResponse, "EnvironmentValidateResponse");
    export_type!(schemas, EnvironmentProposeResponse, "EnvironmentProposeResponse");
    export_type!(schemas, EnvironmentPendingResponse, "EnvironmentPendingResponse");
    export_type!(schemas, EnvironmentStreamEvent, "EnvironmentStreamEvent");
    export_type!(schemas, EnvironmentStreamQuery, "EnvironmentStreamQuery");

    // Component store & runtime
    export_type!(schemas, ComponentStoreQuery, "ComponentStoreQuery");
    export_type!(schemas, ComponentStoreGetResponse, "ComponentStoreGetResponse");
    export_type!(schemas, ComponentStoreSetRequest, "ComponentStoreSetRequest");
    export_type!(schemas, ComponentStoreSetResponse, "ComponentStoreSetResponse");
    export_type!(schemas, ComponentStoreListResponse, "ComponentStoreListResponse");
    export_type!(schemas, ComponentStoreDeleteResponse, "ComponentStoreDeleteResponse");
    export_type!(schemas, ComponentRuntimeEventsRequest, "ComponentRuntimeEventsRequest");
    export_type!(schemas, ComponentRuntimeEventsResponse, "ComponentRuntimeEventsResponse");
    export_type!(schemas, ComponentRuntimeEventsQuery, "ComponentRuntimeEventsQuery");
    export_type!(schemas, ComponentRuntimeEventsTailResponse, "ComponentRuntimeEventsTailResponse");
    export_type!(schemas, ComponentRuntimeProbeResult, "ComponentRuntimeProbeResult");

    // Feeds
    export_type!(schemas, FeedListResponse, "FeedListResponse");
    export_type!(schemas, FeedTailQuery, "FeedTailQuery");
    export_type!(schemas, FeedTailResponse, "FeedTailResponse");
    export_type!(schemas, FeedReadRequest, "FeedReadRequest");
    export_type!(schemas, FeedStreamQuery, "FeedStreamQuery");
    export_type!(schemas, FeedStreamEvent, "FeedStreamEvent");

    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk-contract");
    fs::create_dir_all(&out_dir).expect("create sdk-contract dir");
    let path = out_dir.join("medousa-types.schema.json");
    let json = serde_json::to_string_pretty(&schemas).expect("serialize schemas");
    fs::write(&path, json).expect("write schema file");
    eprintln!("Wrote {} ({} types)", path.display(), schemas.len());
}
