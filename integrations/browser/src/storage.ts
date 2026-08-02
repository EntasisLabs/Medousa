import type { BrowserSettings, PendingContext, PersistedSession } from "./types.js";

export const DEFAULT_ENDPOINT = "http://127.0.0.1:7419";

const SETTINGS_KEY = "settings";
const SESSION_KEY = "session";
const PENDING_CONTEXT_KEY = "pendingContext";
const CLIENT_ID_KEY = "clientId";

export async function loadClientId(): Promise<string> {
  const result = await chrome.storage.local.get(CLIENT_ID_KEY);
  const stored = result[CLIENT_ID_KEY];
  if (typeof stored === "string" && stored.trim()) return stored;
  const clientId = `browser-${crypto.randomUUID()}`;
  await chrome.storage.local.set({ [CLIENT_ID_KEY]: clientId });
  return clientId;
}

export async function loadSettings(): Promise<BrowserSettings> {
  const result = await chrome.storage.local.get(SETTINGS_KEY);
  const stored = result[SETTINGS_KEY];
  if (!stored || typeof stored !== "object") {
    return { endpoint: DEFAULT_ENDPOINT, token: "" };
  }
  const value = stored as Record<string, unknown>;
  return {
    endpoint: typeof value.endpoint === "string" && value.endpoint.trim()
      ? value.endpoint.trim().replace(/\/$/, "")
      : DEFAULT_ENDPOINT,
    token: typeof value.token === "string" ? value.token : "",
  };
}

export async function saveSettings(settings: BrowserSettings): Promise<void> {
  await chrome.storage.local.set({
    [SETTINGS_KEY]: {
      endpoint: settings.endpoint.trim().replace(/\/$/, ""),
      token: settings.token,
    },
  });
}

export async function loadSession(): Promise<PersistedSession> {
  const result = await chrome.storage.local.get(SESSION_KEY);
  const stored = result[SESSION_KEY];
  if (!stored || typeof stored !== "object") {
    return { sessionId: null, sessionName: null };
  }
  const value = stored as Record<string, unknown>;
  return {
    sessionId: typeof value.sessionId === "string" ? value.sessionId : null,
    sessionName: typeof value.sessionName === "string" ? value.sessionName : null,
  };
}

export async function saveSession(session: PersistedSession): Promise<void> {
  await chrome.storage.local.set({ [SESSION_KEY]: session });
}

export async function savePendingContext(pending: PendingContext): Promise<void> {
  await chrome.storage.session.set({ [PENDING_CONTEXT_KEY]: pending });
}

export async function takePendingContext(): Promise<PendingContext | null> {
  const result = await chrome.storage.session.get(PENDING_CONTEXT_KEY);
  await chrome.storage.session.remove(PENDING_CONTEXT_KEY);
  const value = result[PENDING_CONTEXT_KEY];
  if (!value || typeof value !== "object") return null;
  const pending = value as Partial<PendingContext>;
  if (!pending.snapshot || typeof pending.snapshot !== "object") return null;
  if (typeof pending.createdAt !== "number" || Date.now() - pending.createdAt > 60_000) return null;
  return pending as PendingContext;
}
