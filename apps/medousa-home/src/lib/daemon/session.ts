import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ActiveSessionTurnResponse,
  CancelActiveSessionTurnResponse,
  SessionHistoryResponse,
  SessionSetDisplayNameResponse,
  SessionDeleteResponse,
  SessionSummary,
} from "$lib/types/session";
import type {
  AgentModeListResponse,
  AgentModeProposalListResponse,
  AgentModeProposalResponse,
  AgentModeTransitionPolicy,
  SessionAgentModeResponse,
  SessionCodeBindingResponse,
  SessionCodeProjectResponse,
  CreateSessionResponse,
  StartSessionCodeProjectRequest,
  DeriveSessionRequest,
  DeriveSessionResponse,
  CreatePromptStashRequest,
  DeletePromptStashResponse,
  PromptStash,
  PromptStashListResponse,
} from "$lib/types/generated/daemon_api";
import type {
  MediaPathResponse,
  MediaPathPayload,
  MediaPayload,
  MediaPayloadResponse,
  MediaRef,
  MediaUploadResponse,
} from "$lib/types/media";
import type { StageRoutingMatrix } from "$lib/types/runtime";
import { invokePlain, type StreamErrorPayload } from "./client";

export interface InteractiveTurnAccepted {
  turn_id: string;
  stream_url: string;
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

export interface CreateSessionOptions {
  catalog?: "single" | "shared";
  memberProfileIds?: string[];
  agentProfileId?: string;
  displayName?: string;
}

export async function createSession(
  options?: CreateSessionOptions,
): Promise<CreateSessionResponse> {
  return invoke<CreateSessionResponse>("session_create", {
    catalog: options?.catalog,
    memberProfileIds: options?.memberProfileIds,
    agentProfileId: options?.agentProfileId,
    displayName: options?.displayName,
  });
}

export async function deriveSession(
  request: DeriveSessionRequest,
  idempotencyKey: string,
): Promise<DeriveSessionResponse> {
  return invoke<DeriveSessionResponse>("session_derive", {
    request,
    idempotencyKey,
  });
}

export async function listPromptStashes(): Promise<PromptStash[]> {
  const response = await invoke<PromptStashListResponse>("prompt_stash_list");
  return response.stashes;
}

export async function createPromptStash(
  request: CreatePromptStashRequest,
): Promise<PromptStash> {
  return invoke<PromptStash>("prompt_stash_create", {
    request: invokePlain(request),
  });
}

export async function deletePromptStash(
  stashId: string,
): Promise<DeletePromptStashResponse> {
  return invoke<DeletePromptStashResponse>("prompt_stash_delete", { stashId });
}

export interface SharedModeStatus {
  mode: "personal" | "shared" | string;
  enabled_at?: string | null;
  root_profile_id: string;
  general_profile_id: string;
}

export async function getSharedMode(): Promise<SharedModeStatus> {
  return invoke<SharedModeStatus>("shared_mode_status");
}

export async function setSharedMode(
  mode: "personal" | "shared",
): Promise<SharedModeStatus> {
  return invoke<SharedModeStatus>("shared_mode_set", { mode });
}

export interface SessionHistoryOptions {
  limit?: number;
  /** Pagination cursor from a prior `next_cursor` response. */
  cursor?: string;
}

export async function getSessionHistory(
  sessionId: string,
  options?: SessionHistoryOptions,
): Promise<SessionHistoryResponse> {
  return invoke<SessionHistoryResponse>("session_get_history", {
    sessionId,
    limit: options?.limit,
    cursor: options?.cursor?.trim() || undefined,
  });
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

export async function listAgentModes(): Promise<AgentModeListResponse> {
  return invoke<AgentModeListResponse>("agent_mode_list");
}

export async function getAgentModeTransitionPolicy(): Promise<AgentModeTransitionPolicy> {
  return invoke<AgentModeTransitionPolicy>("agent_mode_transition_policy_get");
}

export async function setAgentModeTransitionPolicy(
  policy: AgentModeTransitionPolicy,
): Promise<AgentModeTransitionPolicy> {
  return invoke<AgentModeTransitionPolicy>("agent_mode_transition_policy_set", {
    policy,
  });
}

export async function getSessionAgentMode(
  sessionId: string,
): Promise<SessionAgentModeResponse> {
  return invoke<SessionAgentModeResponse>("session_get_agent_mode", {
    sessionId,
  });
}

export async function setSessionAgentMode(
  sessionId: string,
  mode: import("$lib/types/session").AgentModeId,
): Promise<SessionAgentModeResponse> {
  return invoke<SessionAgentModeResponse>("session_set_agent_mode", {
    sessionId,
    mode,
  });
}

export async function listSessionAgentModeProposals(
  sessionId: string,
): Promise<AgentModeProposalListResponse> {
  return invoke<AgentModeProposalListResponse>("session_list_agent_mode_proposals", {
    sessionId,
  });
}

export async function decideSessionAgentModeProposal(
  sessionId: string,
  proposalId: string,
  accept: boolean,
): Promise<AgentModeProposalResponse> {
  return invoke<AgentModeProposalResponse>("session_decide_agent_mode_proposal", {
    sessionId,
    proposalId,
    accept,
  });
}

export async function getSessionCodeBinding(
  sessionId: string,
): Promise<SessionCodeBindingResponse> {
  return invoke<SessionCodeBindingResponse>("session_get_code_binding", { sessionId });
}

export async function setSessionCodeBinding(
  sessionId: string,
  workId: string,
): Promise<SessionCodeBindingResponse> {
  return invoke<SessionCodeBindingResponse>("session_set_code_binding", { sessionId, workId });
}

export async function clearSessionCodeBinding(
  sessionId: string,
): Promise<SessionCodeBindingResponse> {
  return invoke<SessionCodeBindingResponse>("session_clear_code_binding", { sessionId });
}

export async function startSessionCodeProject(
  sessionId: string,
  request: StartSessionCodeProjectRequest,
): Promise<SessionCodeProjectResponse> {
  return invoke<SessionCodeProjectResponse>("session_start_code_project", {
    sessionId,
    request: invokePlain(request),
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

export async function steerBoundWorkshop(
  sessionId: string,
  workId: string,
  message: string,
): Promise<{ ok: boolean; work_id?: string; error?: string }> {
  return invoke("session_steer_bound_workshop", { sessionId, workId, message });
}

export async function createTurnTicket(
  request: import("$lib/types/session").CreateTurnTicketRequest,
): Promise<import("$lib/types/session").TurnTicketResponse> {
  return invoke<import("$lib/types/session").TurnTicketResponse>("turn_create", {
    sessionId: request.sessionId,
    prompt: request.prompt,
    agentMode: request.agentMode ?? null,
    codeContext: invokePlain(request.codeContext ?? null),
    codeProjectSetupAuthorized: request.codeProjectSetupAuthorized ?? false,
    workerExecutionTarget: invokePlain(request.workerExecutionTarget ?? null),
    mode: request.mode ?? "interactive",
    provider: request.provider ?? null,
    model: request.model ?? null,
    responseDepthMode: request.responseDepthMode ?? null,
    reasoningEffort: request.reasoningEffort ?? null,
    stageRouting: invokePlain(request.stageRouting ?? null),
    channelSurface: request.channelSurface ?? null,
    mediaRefs: invokePlain(request.mediaRefs ?? null),
    voicePresetId: request.voicePresetId ?? null,
    voiceAppendix: request.voiceAppendix ?? null,
    identityUserId: request.identityUserId ?? null,
  });
}

export type AgentRuntimeInfo = {
  runtime: string;
  available: boolean;
  command?: string | null;
  detail?: string | null;
  uses_native_turns?: boolean;
};

export type AgentRuntimeListResponse = {
  runtimes: AgentRuntimeInfo[];
};

export type CodeIntentContext = {
  work_id?: string | null;
  project_title?: string | null;
  outcome?: string | null;
  active_path?: string | null;
  cursor_line?: number | null;
  selection_start_line?: number | null;
  selection_end_line?: number | null;
  selected_text?: string | null;
  containing_symbol?: string | null;
  open_files?: string[];
  diagnostics?: string[];
  last_verification?: string | null;
};

export type CreateAgentSessionRequest = {
  session_id: string;
  runtime: string;
  prompt?: string | null;
  cwd?: string | null;
  command?: string | null;
  args?: string[] | null;
  work_id?: string | null;
  /** ACP wire sessionId to resume (or omit to auto-lookup from work_id). */
  resume_provider_token?: string | null;
  code_context?: CodeIntentContext | null;
};

export type CreateAgentSessionResponse = {
  agent_session_id: string;
  session_id: string;
  runtime: string;
  phase: string;
  stream_url: string;
  stream_ready: boolean;
  accepted_at_utc?: string;
  work_id?: string | null;
  resumed?: boolean | null;
  config_options?: AgentSessionConfigOption[];
};

export type AgentSessionConfigChoice = {
  value: unknown;
  name: string;
  description?: string | null;
};

export type AgentSessionConfigOption = {
  id: string;
  name: string;
  description?: string | null;
  category?: string | null;
  type: string;
  currentValue: unknown;
  options?: AgentSessionConfigChoice[];
};

export type AgentSessionPromptRequest = {
  prompt: string;
  code_context?: CodeIntentContext | null;
};

export async function listAgentRuntimes(): Promise<AgentRuntimeListResponse> {
  return invoke<AgentRuntimeListResponse>("agents_list_runtimes");
}

export async function createAgentSession(
  request: CreateAgentSessionRequest,
): Promise<CreateAgentSessionResponse> {
  return invoke<CreateAgentSessionResponse>("agents_create_session", {
    request: invokePlain(request),
  });
}

export async function promptAgentSession(
  agentSessionId: string,
  prompt: string,
  codeContext?: CodeIntentContext | null,
): Promise<{ accepted: boolean; agent_session_id: string }> {
  return invoke("agents_prompt", {
    agentSessionId,
    request: { prompt, code_context: codeContext ?? undefined },
  });
}

export async function setAgentSessionConfigOption(
  agentSessionId: string,
  configId: string,
  value: unknown,
): Promise<{ agent_session_id: string; config_options: AgentSessionConfigOption[] }> {
  return invoke("agents_set_config_option", {
    agentSessionId,
    request: { config_id: configId, value },
  });
}

export async function cancelAgentSession(
  agentSessionId: string,
): Promise<{ cancelled: boolean; agent_session_id: string; message: string }> {
  return invoke("agents_cancel", { agentSessionId });
}

export async function listAgentPermissionRequests(options?: {
  status?: string;
  limit?: number;
}): Promise<{ requests: unknown[] }> {
  return invoke("agents_list_permission_requests", {
    status: options?.status ?? "pending",
    limit: options?.limit ?? null,
  });
}

export async function approveAgentPermission(
  requestId: string,
  resolvedBy?: string,
): Promise<unknown> {
  return invoke("agents_approve_permission", {
    requestId,
    resolvedBy: resolvedBy ?? null,
  });
}

export async function denyAgentPermission(
  requestId: string,
  resolvedBy?: string,
): Promise<unknown> {
  return invoke("agents_deny_permission", {
    requestId,
    resolvedBy: resolvedBy ?? null,
  });
}

export async function fulfillAgentSecretRequest(
  requestId: string,
  value: string,
  resolvedBy?: string,
): Promise<unknown> {
  return invoke("agents_fulfill_secret_request", {
    requestId,
    value,
    resolvedBy: resolvedBy ?? null,
  });
}

export async function denyAgentSecretRequest(
  requestId: string,
  resolvedBy?: string,
): Promise<unknown> {
  return invoke("agents_deny_secret_request", {
    requestId,
    resolvedBy: resolvedBy ?? null,
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

export interface InteractiveTurnOptions {
  agentMode?: import("$lib/types/session").AgentModeId;
  codeContext?: CodeIntentContext | null;
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
    agentMode: options?.agentMode ?? null,
    codeContext: invokePlain(options?.codeContext ?? null),
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
  bytes: Uint8Array,
  label?: string | null,
): Promise<MediaUploadResponse> {
  return invoke<MediaUploadResponse>("media_upload", {
    sessionId,
    filename,
    mime,
    bytesBase64: bytesToBase64(bytes),
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

export async function readMediaBytes(
  sessionId: string,
  mediaId: string,
): Promise<MediaPayload> {
  const response = await invoke<MediaPayloadResponse>("media_read", { sessionId, mediaId });
  return { mime: response.mime, bytes: base64ToBytes(response.bytes_base64) };
}

export async function readMediaImagePath(path: string): Promise<MediaPathPayload> {
  const response = await invoke<MediaPathResponse>("media_read_image_path", { path });
  return {
    filename: response.filename,
    mime: response.mime,
    bytes: base64ToBytes(response.bytes_base64),
  };
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return btoa(binary);
}

function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
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

export function onInteractiveEvent<T>(
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>("interactive://event", (event) => {
    handler(event.payload);
  });
}

export function onInteractiveError(
  handler: (error: StreamErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<StreamErrorPayload>("interactive://error", (event) => {
    handler(event.payload);
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
