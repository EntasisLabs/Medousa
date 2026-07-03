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

    // Workspace
    export_type!(schemas, WorkspaceCardActionResponse, "WorkspaceCardActionResponse");
    export_type!(schemas, WorkspaceLinkVaultRequest, "WorkspaceLinkVaultRequest");
    export_type!(schemas, WorkspaceStreamEvent, "WorkspaceStreamEvent");

    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk-contract");
    fs::create_dir_all(&out_dir).expect("create sdk-contract dir");
    let path = out_dir.join("medousa-types.schema.json");
    let json = serde_json::to_string_pretty(&schemas).expect("serialize schemas");
    fs::write(&path, json).expect("write schema file");
    eprintln!("Wrote {} ({} types)", path.display(), schemas.len());
}
