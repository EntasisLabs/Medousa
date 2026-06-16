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
  SessionSummary,
} from "$lib/types/session";
import type { ArtifactCommandResponse } from "$lib/types/artifact";
import type { IdentityContextResponse } from "$lib/types/identity";
import type {
  LocusNodeDetailResponse,
  LocusNodesListResponse,
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

export interface DaemonHealth {
  ok: boolean;
  message: string;
  backend?: string | null;
  worker_id?: string | null;
  tool_registry_count?: number | null;
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
    stageRouting: request.stageRouting ?? null,
    channelSurface: request.channelSurface ?? null,
    mediaRefs: request.mediaRefs ?? null,
    voicePresetId: request.voicePresetId ?? null,
    voiceAppendix: request.voiceAppendix ?? null,
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
  stageRouting?: StageRoutingMatrix;
  channelSurface?: string;
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
  command:
    | { command: "model"; args: string[] }
    | { command: "depth"; mode: string | null };
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
    handler(JSON.parse(event.payload) as T);
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
    handler(JSON.parse(event.payload) as T);
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

export async function listLocusNodes(options?: {
  sessionId?: string;
  limit?: number;
  q?: string;
}): Promise<LocusNodesListResponse> {
  return invoke<LocusNodesListResponse>("locus_list_nodes", {
    sessionId: options?.sessionId,
    limit: options?.limit,
    q: options?.q,
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
  manuscript_id?: string;
  timezone?: string;
  execution_mode?: string;
  model_hint?: string;
  policy_profile?: string;
  enabled?: boolean;
  max_attempts?: number;
  queue?: string;
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
      delivery: null,
      session_id: null,
      execution_mode: request.execution_mode ?? null,
      manuscript_id: request.manuscript_id ?? null,
    },
  });
}
