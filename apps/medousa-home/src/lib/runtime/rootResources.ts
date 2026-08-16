/** H09.0 root lifecycle probe — inventory only; Train 1.4 moves the owners. */

export const APP_SHELL_ROOT_RESOURCE_IDS = [
  "wizard-bootstrap",
  "viewport-tracking",
  "native-mobile-layout",
  "mobile-viewport",
  "mobile-native",
  "peer-message-notifications",
  "agent-browser-coord",
  "command-spotlight-hotkeys",
] as const;

export type RootResourceId = (typeof APP_SHELL_ROOT_RESOURCE_IDS)[number];

const live = new Map<RootResourceId, number>();

export function recordRootResource(id: RootResourceId): () => void {
  live.set(id, (live.get(id) ?? 0) + 1);
  return () => {
    const next = (live.get(id) ?? 1) - 1;
    if (next <= 0) live.delete(id);
    else live.set(id, next);
  };
}

export function bindRootResource(id: RootResourceId, stop: () => void): () => void {
  const release = recordRootResource(id);
  return () => {
    stop();
    release();
  };
}

export function listLiveRootResources(): RootResourceId[] {
  return [...live.keys()].sort();
}

export function resetRootResourcesForTests(): void {
  live.clear();
}

/** Eager AppShell graph frozen before Train 1 lazy splits. */
export const APP_SHELL_EAGER_MODULES = [
  "WorkshopShell",
  "MobileShell",
  "CommandSpotlight",
  "WizardContainer",
  "VaultNoteWorkshop",
  "BrowserWorkshop",
  "MobileBrowserWorkshop",
  "$lib/stores/vault.svelte",
  "$lib/stores/lmeWorkspace.svelte",
  "$lib/stores/chat.svelte",
] as const;

export const SHELL_A11Y_FIXTURES = {
  desktop: {
    file: "src/lib/components/layout/WorkshopShell.svelte",
    mustContain: [
      'data-debug-label="app-root"',
      'data-debug-label="workshop-main"',
      "workshop-app-root",
    ],
  },
  mobile: {
    file: "src/lib/components/mobile/MobileShell.svelte",
    mustContain: ['class="mobile-shell', "<main ", "ChatPanel"],
  },
  chat: {
    file: "src/lib/components/chat/ChatPanel.svelte",
    mustContain: ["medousa-chat-composer-focus"],
  },
} as const;
