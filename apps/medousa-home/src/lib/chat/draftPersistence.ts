/** Per-session composer draft persistence. Owned apart from transport/transcript. */

import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";

const DRAFTS_KEY = "medousa-home-chat-drafts";
const DRAFT_MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;
export const DRAFT_PERSIST_DEBOUNCE_MS = 300;

interface StoredDraft {
  text: string;
  updatedAt: number;
}

function draftStorageKey(workshopId?: string): string {
  return workshopScopedStorageKey(DRAFTS_KEY, workshopId);
}

function loadDraftStore(workshopId?: string): Record<string, StoredDraft> {
  if (typeof localStorage === "undefined") return {};
  try {
    const key = draftStorageKey(workshopId);
    const raw = localStorage.getItem(key);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, StoredDraft>;
    if (!parsed || typeof parsed !== "object") return {};
    const now = Date.now();
    const pruned: Record<string, StoredDraft> = {};
    for (const [sessionId, entry] of Object.entries(parsed)) {
      if (!entry || typeof entry.text !== "string") continue;
      if (!entry.text.trim()) continue;
      if (now - (entry.updatedAt ?? 0) > DRAFT_MAX_AGE_MS) continue;
      pruned[sessionId] = entry;
    }
    if (Object.keys(pruned).length !== Object.keys(parsed).length) {
      localStorage.setItem(key, JSON.stringify(pruned));
    }
    return pruned;
  } catch {
    return {};
  }
}

export function loadDraftForSession(sessionId: string, workshopId?: string): string {
  const trimmed = sessionId.trim();
  if (!trimmed) return "";
  return loadDraftStore(workshopId)[trimmed]?.text ?? "";
}

export function persistDraftForSession(
  sessionId: string,
  text: string,
  workshopId?: string,
): void {
  if (typeof localStorage === "undefined") return;
  const trimmed = sessionId.trim();
  if (!trimmed) return;
  const store = loadDraftStore(workshopId);
  if (!text.trim()) {
    delete store[trimmed];
  } else {
    store[trimmed] = { text, updatedAt: Date.now() };
  }
  localStorage.setItem(draftStorageKey(workshopId), JSON.stringify(store));
}

export function clearDraftForSession(sessionId: string, workshopId?: string): void {
  persistDraftForSession(sessionId, "", workshopId);
}
