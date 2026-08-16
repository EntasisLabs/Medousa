/** Shell-owned navigation ports. Feature stores load through these, not AppShell. */

export type PeerThreadInput = {
  workshopId: string;
  peerDeviceId?: string;
  messageId?: string;
};

export type ShellUseCasePorts = {
  openWorkCard?: (cardId: string) => Promise<void>;
  openVaultNote?: (notePath: string) => Promise<void>;
  openPeerThread?: (input: PeerThreadInput) => Promise<void>;
  openCalendarEvent?: (uid: string) => Promise<void>;
  focusChatComposer?: () => void;
};

let ports: ShellUseCasePorts = {};

export function setShellUseCasePortsForTests(next: ShellUseCasePorts): void {
  ports = next;
}

export async function openWorkCard(cardId: string): Promise<void> {
  if (ports.openWorkCard) return ports.openWorkCard(cardId);
  const { layout } = await import("$lib/stores/layout.svelte");
  const { workspace } = await import("$lib/stores/workspace.svelte");
  if (layout.isMobile) {
    layout.setMobileTab("home");
  } else {
    workspace.workView = "hub";
  }
  await workspace.selectCard(cardId);
}

export async function openVaultNote(notePath: string): Promise<void> {
  if (ports.openVaultNote) return ports.openVaultNote(notePath);
  const { layout } = await import("$lib/stores/layout.svelte");
  const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
  layout.navigateDesktop("library");
  await lmeWorkspace.openNote(notePath);
}

export async function openPeerThread(input: PeerThreadInput): Promise<void> {
  if (ports.openPeerThread) return ports.openPeerThread(input);
  const { setPendingPeerNavigation } = await import("$lib/peerNavigation");
  const { layout } = await import("$lib/stores/layout.svelte");
  setPendingPeerNavigation(input.workshopId);
  if (layout.isMobile) {
    layout.openMore("peers");
  } else {
    layout.navigateDesktop("peers", { bump: true });
  }
}

export async function openCalendarEvent(uid: string): Promise<void> {
  if (ports.openCalendarEvent) return ports.openCalendarEvent(uid);
  const { calendar } = await import("$lib/stores/calendar.svelte");
  const { layout } = await import("$lib/stores/layout.svelte");
  if (layout.isMobile) {
    layout.openMore("calendar");
  } else {
    layout.navigateDesktop("calendar", { bump: true });
  }
  await calendar.refresh();
  const match = calendar.events.find((event) => event.uid === uid);
  if (match) calendar.openEdit(match);
}

export function focusChatComposer(): void {
  if (ports.focusChatComposer) {
    ports.focusChatComposer();
    return;
  }
  void import("$lib/stores/layout.svelte").then(({ layout }) => {
    if (layout.isMobile) {
      layout.setMobileTab("chat");
    } else {
      layout.navigateDesktop("chat", { bump: true });
    }
  });
  void import("$lib/stores/chat.svelte").then(({ chat }) => {
    void chat.ensureSessionHydrated();
  });
  window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
}
