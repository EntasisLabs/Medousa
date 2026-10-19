import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";

export interface CompanionDrafts {
  chat: string;
  ask: string;
  mode: "chat" | "ask";
}

const KEY = "medousa-companion-drafts";

export function loadCompanionDrafts(workshopId: string): CompanionDrafts {
  const empty: CompanionDrafts = { chat: "", ask: "", mode: "chat" };
  if (typeof localStorage === "undefined") return empty;
  try {
    const stored = JSON.parse(localStorage.getItem(workshopScopedStorageKey(KEY, workshopId)) ?? "null");
    return {
      chat: typeof stored?.chat === "string" ? stored.chat : "",
      ask: typeof stored?.ask === "string" ? stored.ask : "",
      mode: stored?.mode === "ask" ? "ask" : "chat",
    };
  } catch {
    return empty;
  }
}

export function saveCompanionDrafts(workshopId: string, drafts: CompanionDrafts): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(workshopScopedStorageKey(KEY, workshopId), JSON.stringify(drafts));
}
