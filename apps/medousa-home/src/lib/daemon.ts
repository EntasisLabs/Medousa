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
import type {
  ActiveSessionTurnResponse,
  CancelActiveSessionTurnResponse,
  SessionHistoryResponse,
  SessionSetDisplayNameResponse,
  SessionDeleteResponse,
  SessionSummary,
} from "$lib/types/session";
import type { ArtifactCommandResponse } from "$lib/types/artifact";
import type {
  IdentityContextResponse,
  IdentityDigestPreviewResponse,
  IdentityExportMarkdownResponse,
  IdentityRememberRequest,
  IdentityRememberResponse,
} from "$lib/types/identity";
import type {
  LocusNodeDetailResponse,
  LocusNodesListResponse,
  LocusTagsListResponse,
} from "$lib/types/locus";
import type { EnqueueResponse, JobResultResponse } from "$lib/types/job";
import type {
  DeleteRecurringResponse,
  RecurringListResponse,
  RegisterRecurringResponse,
  UpdateRecurringRequest,
  UpdateRecurringResponse,
} from "$lib/types/recurring";
import type {
  ContinuationStatusResponse,
  DaemonStatsResponse,
  DeliveryHealthResponse,
  RuntimeConfigCommandResponse,
  RuntimeDefaultsResponse,
  StageRouteCommandResponse,
  StageRoutingMatrix,
} from "$lib/types/runtime";
import type {
  VaultBacklinksResponse,
  VaultNoteContentResponse,
  VaultNotesListResponse,
  VaultSearchResponse,
  VaultWriteResponse,
} from "$lib/types/vault";
import type { MediaRef, MediaUploadResponse } from "$lib/types/media";
import type {
  GraphemeAllowlistResponse,
  GraphemeCompileResponse,
  GraphemeLifecycleResponse,
  GraphemeLspWorkspaceResponse,
  GraphemeModuleDetailResponse,
  GraphemeModuleLoadRequest,
  GraphemeModuleLoadResponse,
  GraphemeModulesListResponse,
  GraphemeRunResponse,
  GraphemeScriptDetailResponse,
  GraphemeScriptSaveRequest,
  GraphemeScriptSaveResponse,
  GraphemeScriptsListResponse,
} from "$lib/types/grapheme";
import type {
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

export interface DaemonHealth {
  ok: boolean;
  message: string;
  backend?: string | null;
  worker_id?: string | null;
  tool_registry_count?: number | null;
  agent_runtime_version?: string | null;
  last_agent_turn_at_utc?: string | null;
  last_agent_turn_latency_ms?: number | null;
  active_profile_id?: string | null;
  active_profile_display_name?: string | null;
}

export interface InteractiveTurnAccepted {
  turn_id: string;
  stream_url: string;
}

export async function getDaemonUrl(): Promise<string> {
  return invoke<string>("daemon_url");
}

export async function setDaemonUrl(url: string): Promise<void> {
  return invoke("set_daemon_url", { url });
}

export interface ListSessionsOptions {
  limit?: number;
  /** Home omits TUI verification trust fields for smaller payloads. Default false. */
  includeVerification?: boolean;
  /** Server-side substring search on name, preview, or session id. */
  q?: string;
  /** Pagination cursor from a prior `next_cursor` response. */
  cursor?: string;
}

export interface ListSessionsResponse {
  sessions: SessionSummary[];
  next_cursor?: string | null;
}

export async function listSessions(
  limitOrOptions?: number | ListSessionsOptions,
): Promise<ListSessionsResponse> {
  const options: ListSessionsOptions =
    typeof limitOrOptions === "number"
      ? { limit: limitOrOptions }
      : (limitOrOptions ?? {});
  return invoke<ListSessionsResponse>("session_list", {
    limit: options.limit,
    includeVerification: options.includeVerification ?? false,
    q: options.q?.trim() || undefined,
    cursor: options.cursor?.trim() || undefined,
  });
}

export async function getSessionHistory(
  sessionId: string,
): Promise<SessionHistoryResponse> {
  return invoke<SessionHistoryResponse>("session_get_history", { sessionId });
}

export async function setSessionDisplayName(
  sessionId: string,
  displayName: string,
): Promise<SessionSetDisplayNameResponse> {
  return invoke<SessionSetDisplayNameResponse>("session_set_display_name", {
    sessionId,
    displayName,
  });
}

export async function deleteSession(
  sessionId: string,
  options?: { purgeMemory?: boolean },
): Promise<SessionDeleteResponse> {
  return invoke<SessionDeleteResponse>("session_delete", {
    sessionId,
    purgeMemory: options?.purgeMemory ?? true,
  });
}

export async function getActiveSessionTurn(
  sessionId: string,
): Promise<ActiveSessionTurnResponse> {
  return invoke<ActiveSessionTurnResponse>("session_get_active_turn", {
    sessionId,
  });
}

export async function cancelActiveSessionTurn(
  sessionId: string,
): Promise<CancelActiveSessionTurnResponse> {
  return invoke<CancelActiveSessionTurnResponse>("session_cancel_active_turn", {
    sessionId,
  });
}

export async function createTurnTicket(
  request: import("$lib/types/session").CreateTurnTicketRequest,
): Promise<import("$lib/types/session").TurnTicketResponse> {
  return invoke<import("$lib/types/session").TurnTicketResponse>("turn_create", {
    sessionId: request.sessionId,
    prompt: request.prompt,
    mode: request.mode ?? "interactive",
    provider: request.provider ?? null,
    model: request.model ?? null,
    responseDepthMode: request.responseDepthMode ?? null,
    reasoningEffort: request.reasoningEffort ?? null,
    stageRouting: request.stageRouting ?? null,
    channelSurface: request.channelSurface ?? null,
    mediaRefs: request.mediaRefs ?? null,
    voicePresetId: request.voicePresetId ?? null,
    voiceAppendix: request.voiceAppendix ?? null,
    identityUserId: request.identityUserId ?? null,
  });
}

export async function listSessionTurns(
  sessionId: string,
  activeOnly = true,
): Promise<import("$lib/types/session").SessionTurnsResponse> {
  return invoke<import("$lib/types/session").SessionTurnsResponse>(
    "turn_list_session",
    {
      sessionId,
      activeOnly,
    },
  );
}

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

export async function checkDaemonHealth(): Promise<DaemonHealth> {
  return invoke<DaemonHealth>("daemon_health");
}

export async function startWorkspaceStream(sinceRevision?: number): Promise<void> {
  return invoke("workspace_stream_start", { sinceRevision });
}

export async function stopWorkspaceStream(): Promise<void> {
  return invoke("workspace_stream_stop");
}

export interface InteractiveTurnOptions {
  provider?: string;
  model?: string;
  responseDepthMode?: string;
  reasoningEffort?: string;
  stageRouting?: StageRoutingMatrix;
  channelSurface?: string;
  identityUserId?: string;
}

export async function sendInteractiveTurn(
  sessionId: string,
  prompt: string,
  options?: InteractiveTurnOptions & { mediaRefs?: MediaRef[] },
): Promise<InteractiveTurnAccepted> {
  return invoke<InteractiveTurnAccepted>("interactive_turn_send", {
    sessionId,
    prompt,
    provider: options?.provider,
    model: options?.model,
    responseDepthMode: options?.responseDepthMode,
    reasoningEffort: options?.reasoningEffort,
    stageRouting: options?.stageRouting,
    channelSurface: options?.channelSurface,
  });
}

export async function uploadMediaBytes(
  sessionId: string,
  filename: string,
  mime: string,
  bytes: number[],
  label?: string | null,
): Promise<MediaUploadResponse> {
  return invoke<MediaUploadResponse>("media_upload", {
    sessionId,
    filename,
    mime,
    bytes,
    label: label ?? null,
  });
}

export async function uploadMediaPath(
  sessionId: string,
  path: string,
  label?: string | null,
): Promise<MediaUploadResponse> {
  return invoke<MediaUploadResponse>("media_upload_path", {
    sessionId,
    path,
    label: label ?? null,
  });
}

export async function mediaFetchUrl(sessionId: string, mediaId: string): Promise<string> {
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  const params = new URLSearchParams({ session_id: sessionId });
  return `${base}/v1/media/${encodeURIComponent(mediaId)}?${params.toString()}`;
}

export async function getRuntimeStats(): Promise<DaemonStatsResponse> {
  return invoke<DaemonStatsResponse>("runtime_get_stats");
}

export async function getRuntimeDefaults(): Promise<RuntimeDefaultsResponse> {
  return invoke<RuntimeDefaultsResponse>("runtime_get_defaults");
}

export async function getDeliveryStatus(): Promise<DeliveryHealthResponse> {
  return invoke<DeliveryHealthResponse>("runtime_get_delivery_status");
}

export async function getContinuationStatus(): Promise<ContinuationStatusResponse> {
  return invoke<ContinuationStatusResponse>("runtime_get_continuation_status");
}

export async function sendRuntimeConfigCommand(request: {
  current_provider: string;
  current_model: string;
  draft_provider: string;
  draft_model: string;
  current_response_depth_mode: string;
  current_reasoning_effort?: string;
  command:
    | { command: "model"; args: string[] }
    | { command: "depth"; mode: string | null }
    | { command: "reasoning"; mode: string | null };
}): Promise<RuntimeConfigCommandResponse> {
  return invoke<RuntimeConfigCommandResponse>("runtime_config_command", {
    request,
  });
}

export async function sendStageRouteCommand(request: {
  stage_routing: StageRoutingMatrix;
  provider: string;
  model: string;
  command:
    | { command: "routes"; role: string | null }
    | {
        command: "set";
        role: string;
        target: string;
        policy_profile: string | null;
        fallback_chain: string[] | null;
      }
    | { command: "reset" };
}): Promise<StageRouteCommandResponse> {
  return invoke<StageRouteCommandResponse>("runtime_stage_route_command", {
    request,
  });
}

export async function startInteractiveStream(streamUrl: string): Promise<void> {
  return invoke("interactive_stream_start", { streamUrl });
}

export async function stopInteractiveStream(): Promise<void> {
  return invoke("interactive_stream_stop");
}

export async function stopInteractiveStreamTurn(turnId: string): Promise<void> {
  return invoke("interactive_stream_stop_turn", { turnId });
}

export function onWorkspaceEvent<T>(
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<string>("workspace://event", (event) => {
    try {
      handler(JSON.parse(event.payload) as T);
    } catch {
      // Ignore malformed SSE payloads.
    }
  });
}

export function onWorkspaceError(
  handler: (message: string) => void,
): Promise<UnlistenFn> {
  return listen<{ message: string }>("workspace://error", (event) => {
    handler(event.payload.message);
  });
}

export function onInteractiveEvent<T>(
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<string>("interactive://event", (event) => {
    try {
      handler(JSON.parse(event.payload) as T);
    } catch {
      // Ignore malformed SSE payloads.
    }
  });
}

export function onInteractiveError(
  handler: (message: string) => void,
): Promise<UnlistenFn> {
  return listen<{ message: string }>("interactive://error", (event) => {
    handler(event.payload.message);
  });
}

export async function listVaultNotes(
  prefix?: string,
  limit?: number,
): Promise<VaultNotesListResponse> {
  return invoke<VaultNotesListResponse>("vault_list_notes", { prefix, limit });
}

export async function getVaultNote(
  path: string,
): Promise<VaultNoteContentResponse> {
  return invoke<VaultNoteContentResponse>("vault_get_note", { path });
}

export async function saveVaultNote(
  path: string,
  content: string,
  contentHash?: string,
): Promise<VaultWriteResponse> {
  return invoke<VaultWriteResponse>("vault_save_note", {
    path,
    content,
    contentHash,
  });
}

export async function createVaultNote(
  path: string,
  content: string,
): Promise<VaultWriteResponse> {
  return invoke<VaultWriteResponse>("vault_create_note", { path, content });
}

export async function deleteVaultNote(path: string): Promise<{ path: string; deleted: boolean }> {
  return invoke<{ path: string; deleted: boolean }>("vault_delete_note", { path });
}

export async function searchVaultNotes(
  query: string,
  limit?: number,
): Promise<VaultSearchResponse> {
  return invoke<VaultSearchResponse>("vault_search", { query, limit });
}

export async function getVaultBacklinks(
  path: string,
): Promise<VaultBacklinksResponse> {
  return invoke<VaultBacklinksResponse>("vault_backlinks", { path });
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

export async function approveTurnBudgetRequest(
  requestId: string,
  extraRounds?: number,
  resolvedBy?: string,
): Promise<TurnBudgetRequestResponse> {
  return invoke<TurnBudgetRequestResponse>("turn_budget_approve", {
    requestId,
    extraRounds: extraRounds ?? null,
    resolvedBy: resolvedBy ?? null,
  });
}

export async function denyTurnBudgetRequest(
  requestId: string,
  resolvedBy?: string,
): Promise<TurnBudgetRequestResponse> {
  return invoke<TurnBudgetRequestResponse>("turn_budget_deny", {
    requestId,
    resolvedBy: resolvedBy ?? null,
  });
}

export interface TurnBudgetRequestRecord {
  request_id: string;
  turn_correlation_id?: string | null;
  stream_turn_id: number;
  session_id: string;
  channel?: string | null;
  rounds_executed: number;
  max_tool_rounds: number;
  requested_rounds: number;
  granted_rounds?: number | null;
  reason: string;
  progress_summary?: string | null;
  status: string;
  resolved_by?: string | null;
  created_at_utc: string;
  updated_at_utc: string;
  resolved_at_utc?: string | null;
}

export async function listTurnBudgetRequests(
  pendingOnly = true,
): Promise<TurnBudgetRequestRecord[]> {
  const response = await invoke<{ requests: TurnBudgetRequestRecord[] }>(
    "turn_budget_list",
    { pendingOnly },
  );
  return response.requests ?? [];
}

export interface TurnBudgetRequestResponse {
  request: {
    request_id: string;
    status: string;
    granted_rounds?: number | null;
  };
  message: string;
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
    modelHint: request.modelHint,
    manuscriptId: request.manuscriptId ?? null,
    additionalManuscriptIds: request.additionalManuscriptIds ?? null,
    suggestedCapabilityIds: request.suggestedCapabilityIds ?? null,
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

export async function getIdentityContext(request: {
  user_id?: string;
  persona_id?: string;
  channel_id?: string;
  policy_profile?: string;
  relationship_limit?: number;
  mode?: string;
}): Promise<IdentityContextResponse> {
  return invoke<IdentityContextResponse>("identity_get_context", { request });
}

export async function rememberIdentityFact(
  request: IdentityRememberRequest,
): Promise<IdentityRememberResponse> {
  return invoke("identity_remember", { request });
}

export async function getIdentityDigestPreview(request?: {
  user_id?: string;
  relationship_limit?: number;
  mode?: string;
}): Promise<IdentityDigestPreviewResponse> {
  return invoke("identity_digest_preview", {
    request: {
      mode: request?.mode ?? "cognitive",
      relationship_limit: request?.relationship_limit ?? 32,
      user_id: request?.user_id ?? null,
      persona_id: null,
      channel_id: null,
      policy_profile: null,
    },
  });
}

export async function exportIdentityMarkdown(request?: {
  user_id?: string;
  dir?: string | null;
}): Promise<IdentityExportMarkdownResponse> {
  return invoke("identity_export_markdown", {
    request: {
      user_id: request?.user_id ?? null,
      dir: request?.dir ?? null,
    },
  });
}

export async function listUserProfiles(): Promise<
  import("$lib/types/userProfile").ListUserProfilesResponse
> {
  return invoke("identity_list_profiles");
}

export async function createUserProfile(
  slug: string,
  displayName: string,
): Promise<import("$lib/types/userProfile").CreateUserProfileResponse> {
  return invoke("identity_create_profile", {
    slug,
    displayName,
  });
}

export async function setActiveUserProfile(
  profileId: string,
): Promise<import("$lib/types/userProfile").SetActiveUserProfileResponse> {
  return invoke("identity_set_active_profile", { profileId });
}

export async function listLocusNodes(options?: {
  sessionId?: string;
  limit?: number;
  q?: string;
  tags?: string | string[];
  tagPrefix?: string;
}): Promise<LocusNodesListResponse> {
  const tags =
    options?.tags == null
      ? undefined
      : Array.isArray(options.tags)
        ? options.tags.join(",")
        : options.tags;
  return invoke<LocusNodesListResponse>("locus_list_nodes", {
    sessionId: options?.sessionId,
    limit: options?.limit,
    q: options?.q,
    tags,
    tagPrefix: options?.tagPrefix,
  });
}

export async function listLocusTags(options?: {
  sessionId?: string;
  prefix?: string;
  limit?: number;
}): Promise<LocusTagsListResponse> {
  return invoke<LocusTagsListResponse>("locus_list_tags", {
    sessionId: options?.sessionId,
    prefix: options?.prefix,
    limit: options?.limit,
  });
}

export async function getLocusNode(syncKey: string): Promise<LocusNodeDetailResponse> {
  return invoke<LocusNodeDetailResponse>("locus_get_node", { syncKey });
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

export async function updateRecurring(
  recurringId: string,
  request: UpdateRecurringRequest,
): Promise<UpdateRecurringResponse> {
  return invoke<UpdateRecurringResponse>("recurring_update", {
    recurringId,
    request,
  });
}

export async function deleteRecurring(
  recurringId: string,
): Promise<DeleteRecurringResponse> {
  return invoke<DeleteRecurringResponse>("recurring_delete", { recurringId });
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
      cron_expr: request.cron_expr,
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

export function daemonWebSocketUrl(path: string): Promise<string> {
  return getDaemonUrl().then((base) => {
    const normalized = base.replace(/\/$/, "");
    const wsBase = normalized.replace(/^http/i, "ws");
    return `${wsBase}${path.startsWith("/") ? path : `/${path}`}`;
  });
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
