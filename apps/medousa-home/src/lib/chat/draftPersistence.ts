/** Per-session composer draft persistence. Owned apart from transport/transcript. */

const DRAFTS_KEY = "medousa-home-chat-drafts";
const DRAFT_MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;
export const DRAFT_PERSIST_DEBOUNCE_MS = 300;

interface StoredDraft {
  text: string;
  updatedAt: number;
}

function loadDraftStore(): Record<string, StoredDraft> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(DRAFTS_KEY);
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
      localStorage.setItem(DRAFTS_KEY, JSON.stringify(pruned));
    }
    return pruned;
  } catch {
    return {};
  }
}

export function loadDraftForSession(sessionId: string): string {
  const trimmed = sessionId.trim();
  if (!trimmed) return "";
  return loadDraftStore()[trimmed]?.text ?? "";
}

export function persistDraftForSession(sessionId: string, text: string): void {
  if (typeof localStorage === "undefined") return;
  const trimmed = sessionId.trim();
  if (!trimmed) return;
  const store = loadDraftStore();
  if (!text.trim()) {
    delete store[trimmed];
  } else {
    store[trimmed] = { text, updatedAt: Date.now() };
  }
  localStorage.setItem(DRAFTS_KEY, JSON.stringify(store));
}

export function clearDraftForSession(sessionId: string): void {
  persistDraftForSession(sessionId, "");
}
