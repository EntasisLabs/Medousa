import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";
export interface ChatPopoutRequest { sessionId: string; scope: string; requestedAt: number }
export const CHAT_POPOUT_KEY = "medousa-chat-popout-request";
export function writeChatPopoutRequest(sessionId: string, scope: string): ChatPopoutRequest {
  const request = { sessionId, scope, requestedAt: Date.now() };
  try { localStorage.setItem(workshopScopedStorageKey(CHAT_POPOUT_KEY, scope), JSON.stringify(request)); } catch { /* The live window also receives an event. */ }
  return request;
}
export function readChatPopoutRequest(scope: string): ChatPopoutRequest | null {
  try {
    const value = JSON.parse(localStorage.getItem(workshopScopedStorageKey(CHAT_POPOUT_KEY, scope)) ?? "null");
    if (value?.scope !== scope || typeof value.sessionId !== "string" || !value.sessionId.trim() || typeof value.requestedAt !== "number" || Math.abs(Date.now() - value.requestedAt) > 120_000) return null;
    return value;
  } catch { return null; }
}
export function clearChatPopoutRequest(request: ChatPopoutRequest): void {
  try {
    const pending = readChatPopoutRequest(request.scope);
    if (pending?.requestedAt === request.requestedAt && pending.sessionId === request.sessionId) localStorage.removeItem(workshopScopedStorageKey(CHAT_POPOUT_KEY, request.scope));
  } catch { /* Best effort local navigation handoff. */ }
}
