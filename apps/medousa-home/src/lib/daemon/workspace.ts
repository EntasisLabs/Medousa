import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { WorkCardDetail } from "$lib/types/card";
import type { WorkspaceCardActionResponse } from "$lib/types/work";
import type { WorkspaceSnapshot } from "$lib/types/workspace";
import type {
  CapabilityListResponse,
  CapabilityResolveResponse,
  ManuscriptCatalogResponse,
} from "$lib/types/catalog";
import type { ArtifactCommandResponse } from "$lib/types/artifact";
import type { EnqueueResponse, JobResultResponse } from "$lib/types/job";
import type {
  DeleteRecurringResponse,
  RecurringListResponse,
  RegisterRecurringResponse,
  UpdateRecurringRequest,
  UpdateRecurringResponse,
} from "$lib/types/recurring";
import { toSevenFieldCron } from "$lib/utils/friendlySchedule";
import type {
  GraphemeAllowlistResponse,
  GraphemeCompileResponse,
  GraphemeLifecycleResponse,
  GraphemeLspWorkspaceResponse,
  GraphemeModuleDetailResponse,
  GraphemeModuleLoadRequest,
  GraphemeModuleLoadResponse,
  GraphemeModuleOpsResponse,
  GraphemeModulesListResponse,
  GraphemeRunResponse,
  GraphemeScriptDeleteResponse,
  GraphemeScriptDetailResponse,
  GraphemeScriptSaveRequest,
  GraphemeScriptSaveResponse,
  GraphemeScriptsListResponse,
} from "$lib/types/grapheme";
import type {
  CreateManuscriptRequest,
  ManuscriptDetailResponse,
  ManuscriptImportRequest,
  ManuscriptImportResponse,
  UpdateManuscriptRequest,
} from "$lib/types/manuscript";
import type {
  WorkflowDetailResponse,
  WorkflowPlanRequest,
  WorkflowPlanResponse,
  WorkflowRunRequest,
  WorkflowRunResponse,
  WorkflowRunsResponse,
  WorkflowScheduleRequest,
  WorkflowScheduleResponse,
  WorkflowsListResponse,
} from "$lib/types/workflow";
import type {
  ToolHistoryListResponse,
  WorkflowFromSliceRequest,
  WorkflowFromSliceResponse,
} from "$lib/types/toolHistory";
import { invokePlain, type StreamErrorPayload } from "./client";

export async function listManuscripts(options?: {
  prefix?: string;
  limit?: number;
  skillsOnly?: boolean;
}): Promise<ManuscriptCatalogResponse> {
  return invoke<ManuscriptCatalogResponse>("catalog_list_manuscripts", {
    prefix: options?.prefix,
    limit: options?.limit,
    skillsOnly: options?.skillsOnly,
  });
}

export async function listCapabilities(): Promise<CapabilityListResponse> {
  return invoke<CapabilityListResponse>("catalog_list_capabilities");
}

export async function getCapability(
  capabilityId: string,
): Promise<CapabilityResolveResponse> {
  return invoke<CapabilityResolveResponse>("catalog_get_capability", {
    capabilityId,
  });
}

export async function getManuscript(
  manuscriptId: string,
): Promise<ManuscriptDetailResponse> {
  return invoke<ManuscriptDetailResponse>("catalog_get_manuscript", {
    manuscriptId,
  });
}

export async function createManuscript(
  request: CreateManuscriptRequest,
): Promise<ManuscriptDetailResponse> {
  return invoke<ManuscriptDetailResponse>("catalog_create_manuscript", {
    request,
  });
}

export async function updateManuscript(
  manuscriptId: string,
  request: UpdateManuscriptRequest,
): Promise<ManuscriptDetailResponse> {
  return invoke<ManuscriptDetailResponse>("catalog_update_manuscript", {
    manuscriptId,
    request,
  });
}

export async function importManuscripts(
  request: ManuscriptImportRequest,
): Promise<ManuscriptImportResponse> {
  return invoke<ManuscriptImportResponse>("catalog_import_manuscripts", {
    request,
  });
}

export async function startWorkspaceStream(sinceRevision?: number): Promise<void> {
  return invoke("workspace_stream_start", { sinceRevision });
}

export async function stopWorkspaceStream(): Promise<void> {
  return invoke("workspace_stream_stop");
}

export function onWorkspaceEvent<T>(
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>("workspace://event", (event) => {
    handler(event.payload);
  });
}

export function onWorkspaceError(
  handler: (error: StreamErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<StreamErrorPayload>("workspace://error", (event) => {
    handler(event.payload);
  });
}

export async function getWorkspaceCard(
  cardId: string,
): Promise<WorkCardDetail> {
  return invoke<WorkCardDetail>("workspace_get_card", { cardId });
}

export async function fetchWorkspaceSnapshot(
  sinceRevision?: number,
): Promise<WorkspaceSnapshot> {
  return invoke<WorkspaceSnapshot>("workspace_fetch_snapshot", {
    sinceRevision,
  });
}

export async function archiveWorkspaceCard(
  cardId: string,
  purgeOutput = true,
): Promise<WorkspaceCardActionResponse> {
  return invoke<WorkspaceCardActionResponse>("workspace_archive_card", {
    cardId,
    purgeOutput,
  });
}

export async function cancelWorkspaceCard(
  cardId: string,
): Promise<WorkspaceCardActionResponse> {
  return invoke<WorkspaceCardActionResponse>("workspace_cancel_card", {
    cardId,
  });
}

export async function retryWorkspaceCard(
  cardId: string,
): Promise<WorkspaceCardActionResponse> {
  return invoke<WorkspaceCardActionResponse>("workspace_retry_card", {
    cardId,
  });
}

export async function getJobResult(jobId: string): Promise<JobResultResponse> {
  return invoke<JobResultResponse>("job_get_result", { jobId });
}

export async function completeAskJobActions(
  jobId: string,
  request: {
    writeJournalPath?: string;
    notifyChannel?: string;
  } = {},
): Promise<import("$lib/types/askJob").AskJobCompleteActionsResponse> {
  return invoke("job_complete_actions", {
    jobId,
    writeJournalPath: request.writeJournalPath ?? null,
    notifyChannel: request.notifyChannel ?? null,
  });
}

export async function archiveAskJob(
  jobId: string,
  purgeOutput = true,
): Promise<import("$lib/types/askJob").ArchiveAskJobResponse> {
  return invoke("job_archive_ask", { jobId, purgeOutput });
}

export interface EnqueueDaemonAskRequest {
  prompt: string;
  modelHint?: string;
  manuscriptId?: string;
  additionalManuscriptIds?: string[];
  suggestedCapabilityIds?: string[];
}

export async function enqueueDaemonAsk(
  request: EnqueueDaemonAskRequest | string,
  modelHint?: string,
): Promise<EnqueueResponse> {
  if (typeof request === "string") {
    return invoke<EnqueueResponse>("job_enqueue_ask", {
      prompt: request,
      modelHint,
      manuscriptId: null,
      additionalManuscriptIds: null,
      suggestedCapabilityIds: null,
    });
  }

  return invoke<EnqueueResponse>("job_enqueue_ask", {
    prompt: request.prompt,
    modelHint: request.modelHint ?? null,
    manuscriptId: request.manuscriptId ?? null,
    additionalManuscriptIds: invokePlain(request.additionalManuscriptIds ?? null),
    suggestedCapabilityIds: invokePlain(request.suggestedCapabilityIds ?? null),
  });
}

export async function listRecurringRuns(
  recurringId: string,
  limit?: number,
): Promise<import("$lib/types/recurring").RecurringRunsResponse> {
  return invoke("recurring_list_runs", { recurringId, limit: limit ?? null });
}

export async function getRecurringDelivery(
  recurringId: string,
): Promise<import("$lib/types/recurring").RecurringDeliveryResponse> {
  return invoke("recurring_get_delivery", { recurringId });
}

export async function listRecurring(
  enabledOnly?: boolean,
): Promise<RecurringListResponse> {
  return invoke<RecurringListResponse>("recurring_list", { enabledOnly });
}

export async function lookupArtifact(
  sessionId: string,
  artifactId: string,
): Promise<ArtifactCommandResponse> {
  return invoke<ArtifactCommandResponse>("artifact_command", {
    request: {
      session_id: sessionId,
      selected_context_pack_query: null,
      command: { command: "lookup", query: artifactId },
    },
  });
}

export async function fetchArtifact(
  sessionId: string,
  artifactId: string,
): Promise<import("$lib/types/artifact").ArtifactFetchResponse> {
  return invoke<import("$lib/types/artifact").ArtifactFetchResponse>("artifact_fetch", {
    request: {
      session_id: sessionId,
      artifact_id: artifactId,
    },
  });
}

export async function listUiArtifacts(options?: {
  sessionId?: string;
  query?: string;
  limit?: number;
}): Promise<import("$lib/types/artifact").ArtifactListUiResponse> {
  return invoke<import("$lib/types/artifact").ArtifactListUiResponse>("artifact_list_ui", {
    request: {
      session_id: options?.sessionId ?? null,
      query: options?.query ?? null,
      limit: options?.limit ?? 50,
    },
  });
}

export async function writeArtifact(
  request: import("$lib/types/artifact").ArtifactWriteRequest,
): Promise<import("$lib/types/artifact").ArtifactWriteResponse> {
  return invoke<import("$lib/types/artifact").ArtifactWriteResponse>("artifact_write", {
    request,
  });
}

export async function deleteArtifact(
  request: import("$lib/types/artifact").ArtifactDeleteRequest,
): Promise<import("$lib/types/artifact").ArtifactDeleteResponse> {
  return invoke<import("$lib/types/artifact").ArtifactDeleteResponse>("artifact_delete", {
    request,
  });
}

export async function updateRecurring(
  recurringId: string,
  request: UpdateRecurringRequest,
): Promise<UpdateRecurringResponse> {
  const next =
    request.cron_expr != null
      ? { ...request, cron_expr: toSevenFieldCron(request.cron_expr) }
      : request;
  return invoke<UpdateRecurringResponse>("recurring_update", {
    recurringId,
    request: next,
  });
}

export async function deleteRecurring(
  recurringId: string,
): Promise<DeleteRecurringResponse> {
  return invoke<DeleteRecurringResponse>("recurring_delete", { recurringId });
}

export interface ArtifactRetentionStatus {
  settings: {
    enabled: boolean;
    max_age_days: number;
    max_per_session: number;
    recurring_id: string;
    cron_expr: string;
  };
  scheduled: boolean;
  enabled: boolean;
  next_run_at_utc: string | null;
  last_run_at_utc: string | null;
  last_run_summary: string | null;
}

export async function getArtifactRetentionStatus(): Promise<ArtifactRetentionStatus> {
  return invoke<ArtifactRetentionStatus>("artifact_retention_status");
}

export async function updateArtifactRetention(request: {
  enabled?: boolean;
  max_age_days?: number;
  max_per_session?: number;
}): Promise<{
  settings: ArtifactRetentionStatus["settings"];
  next_run_at_utc: string;
}> {
  return invoke("artifact_retention_update", { request });
}

export async function registerRecurringPrompt(request: {
  prompt: string;
  cron_expr: string;
  display_name?: string;
  manuscript_id?: string;
  timezone?: string;
  execution_mode?: string;
  model_hint?: string;
  policy_profile?: string;
  enabled?: boolean;
  max_attempts?: number;
  queue?: string;
  delivery?: Record<string, unknown> | null;
}): Promise<RegisterRecurringResponse> {
  return invoke<RegisterRecurringResponse>("recurring_register_prompt", {
    request: {
      id: null,
      queue: request.queue ?? "default",
      prompt: request.prompt,
      system_prompt:
        "Medousa runtime collaborator — evidence-led, concise, warm continuity. The principal owns the workspace; honor AVEC, STTP, and continuity blocks when present. Tool receipts ground claims.",
      cron_expr: toSevenFieldCron(request.cron_expr),
      timezone: request.timezone ?? "UTC",
      jitter_seconds: 0,
      enabled: request.enabled ?? true,
      max_attempts: request.max_attempts ?? 1,
      policy_profile: request.policy_profile ?? "scheduled",
      model_hint: request.model_hint ?? null,
      delivery: request.delivery ?? null,
      session_id: null,
      execution_mode: request.execution_mode ?? "agent_turn",
      manuscript_id: request.manuscript_id ?? null,
      display_name: request.display_name ?? null,
    },
  });
}

export async function listGraphemeModules(): Promise<GraphemeModulesListResponse> {
  return invoke<GraphemeModulesListResponse>("grapheme_list_modules");
}

export async function getGraphemeModule(
  moduleId: string,
): Promise<GraphemeModuleDetailResponse> {
  return invoke<GraphemeModuleDetailResponse>("grapheme_get_module", {
    moduleId,
  });
}

export async function getGraphemeModuleOps(
  moduleId: string,
  query?: string,
): Promise<GraphemeModuleOpsResponse> {
  return invoke<GraphemeModuleOpsResponse>("grapheme_get_module_ops", {
    moduleId,
    q: query?.trim() || null,
  });
}

export async function listGraphemeScripts(options?: {
  query?: string;
  module?: string;
  tag?: string;
  limit?: number;
}): Promise<GraphemeScriptsListResponse> {
  return invoke<GraphemeScriptsListResponse>("grapheme_list_scripts", {
    query: options?.query ?? null,
    module: options?.module ?? null,
    tag: options?.tag ?? null,
    limit: options?.limit ?? null,
  });
}

export async function getGraphemeScript(
  scriptId: string,
): Promise<GraphemeScriptDetailResponse> {
  return invoke<GraphemeScriptDetailResponse>("grapheme_get_script", {
    scriptId,
  });
}

export async function runGraphemeSource(
  source: string,
): Promise<GraphemeRunResponse> {
  return invoke<GraphemeRunResponse>("grapheme_run_source", { source });
}

export async function getGraphemeAllowlist(): Promise<GraphemeAllowlistResponse> {
  return invoke<GraphemeAllowlistResponse>("grapheme_get_allowlist");
}

export async function updateGraphemeAllowlist(
  allowedModules: string[],
): Promise<GraphemeAllowlistResponse> {
  return invoke<GraphemeAllowlistResponse>("grapheme_update_allowlist", {
    allowedModules,
  });
}

export async function saveGraphemeScript(
  request: GraphemeScriptSaveRequest,
): Promise<GraphemeScriptSaveResponse> {
  return invoke<GraphemeScriptSaveResponse>("grapheme_save_script", { request });
}

export async function deleteGraphemeScript(
  scriptId: string,
): Promise<GraphemeScriptDeleteResponse> {
  return invoke<GraphemeScriptDeleteResponse>("grapheme_delete_script", {
    scriptId,
  });
}

export async function renameGraphemeScript(
  scriptId: string,
  name: string,
): Promise<GraphemeScriptSaveResponse> {
  return invoke<GraphemeScriptSaveResponse>("grapheme_rename_script", {
    scriptId,
    name,
  });
}

export async function compileGraphemeSource(
  source: string,
  mode?: string,
): Promise<GraphemeCompileResponse> {
  return invoke<GraphemeCompileResponse>("grapheme_compile_source", {
    source,
    mode: mode ?? null,
  });
}

export async function loadGraphemeModule(
  request: GraphemeModuleLoadRequest,
): Promise<GraphemeModuleLoadResponse> {
  return invoke<GraphemeModuleLoadResponse>("grapheme_load_module", { request });
}

export async function getGraphemeLifecycle(): Promise<GraphemeLifecycleResponse> {
  return invoke<GraphemeLifecycleResponse>("grapheme_get_lifecycle");
}

export async function getGraphemeLspWorkspace(): Promise<GraphemeLspWorkspaceResponse> {
  return invoke<GraphemeLspWorkspaceResponse>("grapheme_get_lsp_workspace");
}

export type CodingEngineInfoResponse = {
  available: boolean;
  url: string;
  health_url: string;
  lsp_url: string;
  daemon_lsp_path: string;
  workspace_root: string;
  workspace_root_uri: string;
  bind: string;
  message: string;
};

export async function getCodingEngineInfo(): Promise<CodingEngineInfoResponse> {
  return invoke<CodingEngineInfoResponse>("coding_engine_info");
}

export async function listWorkflows(
  limit?: number,
): Promise<WorkflowsListResponse> {
  return invoke<WorkflowsListResponse>("workflow_list", { limit: limit ?? null });
}

export async function getWorkflow(
  workflowId: string,
): Promise<WorkflowDetailResponse> {
  return invoke<WorkflowDetailResponse>("workflow_get", { workflowId });
}

export async function runWorkflow(
  request: WorkflowRunRequest,
): Promise<WorkflowRunResponse> {
  return invoke<WorkflowRunResponse>("workflow_run", { request });
}

export async function planWorkflow(
  request: WorkflowPlanRequest,
): Promise<WorkflowPlanResponse> {
  return invoke<WorkflowPlanResponse>("workflow_plan", { request });
}

export async function scheduleWorkflow(
  request: WorkflowScheduleRequest,
): Promise<WorkflowScheduleResponse> {
  return invoke<WorkflowScheduleResponse>("workflow_schedule", { request });
}

export async function listWorkflowRuns(
  workflowId: string,
  limit?: number,
): Promise<WorkflowRunsResponse> {
  return invoke<WorkflowRunsResponse>("workflow_list_runs", {
    workflowId,
    limit: limit ?? null,
  });
}

export async function listToolHistorySlices(options?: {
  limit?: number;
  sessionLimit?: number;
  sessionId?: string;
  toolFilter?: string;
  keyword?: string;
}): Promise<ToolHistoryListResponse> {
  return invoke<ToolHistoryListResponse>("tool_history_list_slices", {
    limit: options?.limit ?? null,
    sessionLimit: options?.sessionLimit ?? null,
    sessionId: options?.sessionId ?? null,
    toolFilter: options?.toolFilter ?? null,
    keyword: options?.keyword ?? null,
  });
}

export async function promoteWorkflowFromSlice(
  request: WorkflowFromSliceRequest,
): Promise<WorkflowFromSliceResponse> {
  return invoke<WorkflowFromSliceResponse>("workflow_from_slice", { request });
}
