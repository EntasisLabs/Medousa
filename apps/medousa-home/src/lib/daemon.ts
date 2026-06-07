import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { WorkCardDetail } from "$lib/types/card";
import type { WorkspaceCardActionResponse } from "$lib/types/work";
import type {
  SessionHistoryResponse,
  SessionSummary,
} from "$lib/types/session";
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

export async function checkDaemonHealth(): Promise<DaemonHealth> {
  return invoke<DaemonHealth>("daemon_health");
}

export async function startWorkspaceStream(sinceRevision?: number): Promise<void> {
  return invoke("workspace_stream_start", { sinceRevision });
}

export async function stopWorkspaceStream(): Promise<void> {
  return invoke("workspace_stream_stop");
}

export async function sendInteractiveTurn(
  sessionId: string,
  prompt: string,
): Promise<InteractiveTurnAccepted> {
  return invoke<InteractiveTurnAccepted>("interactive_turn_send", {
    sessionId,
    prompt,
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
