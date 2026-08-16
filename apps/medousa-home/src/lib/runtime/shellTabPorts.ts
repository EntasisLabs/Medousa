/** Ports so shell tabs orchestrate features without importing sibling stores. */

import type { ChatMessage } from "$lib/types/chat";
import type { SessionSummary } from "$lib/types/session";
import type { LmeTab, LmeWorkspaceSession } from "$lib/stores/lmeWorkspace.svelte";

export type ShellTabChatPort = {
  sessionId: () => string;
  sessions: () => SessionSummary[];
  messagesFor: (sessionId: string) => ChatMessage[];
  historyLoadingFor: (sessionId: string) => boolean;
  warmBackgroundSession: (sessionId: string) => void;
  switchSession: (sessionId: string) => Promise<void>;
  newSession: (options?: { shellContext?: { desktopId: string; groupId: string } }) => void;
};

export type ShellTabLmePort = {
  tabs: () => LmeTab[];
  activeTab: () => LmeTab | null;
  activeTabId: () => string | null;
  captureSession: () => LmeWorkspaceSession;
  restoreSession: (value: unknown) => LmeWorkspaceSession;
  activateTab: (tabId: string) => Promise<void>;
  closeTab: (
    tabId: string,
    options?: { activateNext?: boolean; confirmed?: boolean },
  ) => Promise<void>;
  confirmCloseTab: (tabId: string) => boolean;
};

export type ShellTabVaultPort = {
  flushBeforeLeave: () => Promise<boolean>;
  openNote: (path: string) => Promise<void>;
  isFocusedPath: (path: string) => boolean;
};

export type ShellTabBrowserPort = {
  tabs: () => Array<{ id: string; title: string; url: string }>;
  activeTab: () => { id: string; title: string; url: string } | null;
  activateTab: (tabId: string) => Promise<void>;
  closeTab: (tabId: string) => void;
  openTab: (url: string) => Promise<void>;
};

export type ShellTabCodePort = {
  resetForWorkshopSwitch: () => void;
};

export type ShellTabFeaturePorts = {
  chat: ShellTabChatPort;
  lme: ShellTabLmePort;
  vault: ShellTabVaultPort;
  browser: ShellTabBrowserPort;
  code: ShellTabCodePort;
};

const unbound: ShellTabFeaturePorts = {
  chat: {
    sessionId: () => "",
    sessions: () => [],
    messagesFor: () => [],
    historyLoadingFor: () => false,
    warmBackgroundSession: () => {},
    switchSession: async () => {},
    newSession: () => {},
  },
  lme: {
    tabs: () => [],
    activeTab: () => null,
    activeTabId: () => null,
    captureSession: () => ({ tabs: [], activeTabId: null }),
    restoreSession: () => ({ tabs: [], activeTabId: null }),
    activateTab: async () => {},
    closeTab: async () => {},
    confirmCloseTab: () => true,
  },
  vault: {
    flushBeforeLeave: async () => true,
    openNote: async () => {},
    isFocusedPath: () => true,
  },
  browser: {
    tabs: () => [],
    activeTab: () => null,
    activateTab: async () => {},
    closeTab: () => {},
    openTab: async () => {},
  },
  code: {
    resetForWorkshopSwitch: () => {},
  },
};

let ports: ShellTabFeaturePorts | null = null;

export function setShellTabPorts(next: ShellTabFeaturePorts | null): void {
  ports = next;
}

export function shellTabFeaturePorts(): ShellTabFeaturePorts {
  return ports ?? unbound;
}
