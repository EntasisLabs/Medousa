import { isExternalRuntime } from "$lib/utils/externalAgentCatalog";
import type { ExternalAgentRuntime } from "$lib/utils/externalAgentCatalog";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";
import type { ConversationTarget } from "$lib/spotlight/conversationTargets";
import { loadCompanionDrafts } from "./companionDrafts";

export type SavedAddress = { label: string } & (
  { kind: "bot"; botId: string } |
  { kind: "session"; sessionId: string } |
  { kind: "new"; runtime: "medousa" | ExternalAgentRuntime; sessionId: string | null }
);
export interface AddressedDraft { query: string; address: SavedAddress | null }
const KEY = "medousa-companion-conversation";
function address(value: unknown): SavedAddress | null {
  if (!value || typeof value !== "object") return null;
  const item = value as Record<string, unknown>;
  if (typeof item.label !== "string" || !item.label.trim()) return null;
  if (item.kind === "bot" && typeof item.botId === "string" && item.botId) return { kind: "bot", botId: item.botId, label: item.label };
  if (item.kind === "session" && typeof item.sessionId === "string" && item.sessionId) return { kind: "session", sessionId: item.sessionId, label: item.label };
  if (item.kind === "new" && (item.runtime === "medousa" || isExternalRuntime(item.runtime)) && (item.sessionId === null || typeof item.sessionId === "string")) {
    return { kind: "new", label: item.label, runtime: item.runtime as "medousa" | ExternalAgentRuntime, sessionId: item.sessionId || null };
  }
  return null;
}
export function loadAddressedDraft(scope: string): AddressedDraft {
  const empty = { query: "", address: null };
  if (typeof localStorage === "undefined") return empty;
  try {
    const raw = localStorage.getItem(workshopScopedStorageKey(KEY, scope));
    if (raw !== null) {
      const stored = JSON.parse(raw);
      // An invalid recipient must never turn a draft into an unaddressed send.
      if (stored?.address != null && !address(stored.address)) return empty;
      return { query: typeof stored?.query === "string" ? stored.query : "", address: address(stored?.address) };
    }
    const legacy = loadCompanionDrafts(scope);
    return { query: legacy[legacy.mode], address: null };
  } catch { return empty; }
}
export function saveAddressedDraft(scope: string, draft: AddressedDraft): void {
  if (typeof localStorage === "undefined") return;
  try { localStorage.setItem(workshopScopedStorageKey(KEY, scope), JSON.stringify(draft)); }
  catch { /* A full or unavailable client cache must not prevent sending. */ }
}
export function savedAddress(target: ConversationTarget | null, sessionId: string | null): SavedAddress | null {
  if (!target) return null;
  const label = target.label;
  if (target.kind === "bot") return { kind: "bot", label, botId: target.bot.bot_id };
  if (target.kind === "session") return { kind: "session", label, sessionId: target.sessionId };
  return { kind: "new", label, runtime: target.runtime, sessionId };
}
