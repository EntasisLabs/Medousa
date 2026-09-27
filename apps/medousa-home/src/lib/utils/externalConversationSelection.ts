import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";
import type { ExternalProvider } from "$lib/daemon/externalConversations";

const STORAGE_KEY = "medousa-external-conversation-selection-v1";

function key(sessionId: string, provider: ExternalProvider): string {
  return `${sessionId.trim()}:${provider}`;
}

function read(): Record<string, string> {
  if (typeof localStorage === "undefined") return {};
  try {
    const value = JSON.parse(localStorage.getItem(workshopScopedStorageKey(STORAGE_KEY)) ?? "{}");
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    return value as Record<string, string>;
  } catch {
    return {};
  }
}

export function getExternalConversationSelection(sessionId: string, provider: ExternalProvider): string | null {
  const value = read()[key(sessionId, provider)];
  return typeof value === "string" && value.trim() ? value : null;
}

export function setExternalConversationSelection(sessionId: string, provider: ExternalProvider, id: string): void {
  if (typeof localStorage === "undefined" || !sessionId.trim() || !id.trim()) return;
  const all = read();
  all[key(sessionId, provider)] = id.trim();
  localStorage.setItem(workshopScopedStorageKey(STORAGE_KEY), JSON.stringify(all));
}
