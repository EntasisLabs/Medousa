import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { WorkCardDetail } from "$lib/types/card";
import type { WorkspaceCardActionResponse } from "$lib/types/work";
import type {
  CapabilityListResponse,
  ManuscriptCatalogResponse,
} from "$lib/types/catalog";
import type {
  SessionHistoryResponse,
  SessionSummary,
} from "$lib/types/session";
import type {
  ContinuationStatusResponse,
  DaemonStatsResponse,
  DeliveryHealthResponse,
  RuntimeConfigCommandResponse,
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

export async function listSessions(
  limit?: number,
): Promise<{ sessions: SessionSummary[] }> {
  return invoke<{ sessions: SessionSummary[] }>("session_list", { limit });
}

export async function getSessionHistory(
  sessionId: string,
): Promise<SessionHistoryResponse> {
  return invoke<SessionHistoryResponse>("session_get_history", { sessionId });
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
}

export async function sendInteractiveTurn(
  sessionId: string,
  prompt: string,
  options?: InteractiveTurnOptions,
): Promise<InteractiveTurnAccepted> {
  return invoke<InteractiveTurnAccepted>("interactive_turn_send", {
    sessionId,
    prompt,
    provider: options?.provider,
    model: options?.model,
    responseDepthMode: options?.responseDepthMode,
    stageRouting: options?.stageRouting,
  });
}

export async function getRuntimeStats(): Promise<DaemonStatsResponse> {
  return invoke<DaemonStatsResponse>("runtime_get_stats");
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
