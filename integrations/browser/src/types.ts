export interface BrowserPageSnapshot {
  tabId?: number;
  windowId?: number;
  title: string;
  url: string;
  selection: string;
  text: string;
}

export interface PendingContext {
  snapshot: BrowserPageSnapshot;
  prompt?: string;
  createdAt: number;
}

export interface BrowserSettings {
  endpoint: string;
  token: string;
}

export interface PersistedSession {
  sessionId: string | null;
  sessionName: string | null;
}

export type BrowserChatMessage = {
  role: "user" | "assistant" | "error";
  content: string;
  contextLabel?: string;
};
