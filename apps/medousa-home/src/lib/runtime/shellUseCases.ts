/** Shell-owned navigation ports. Feature stores load through these, not AppShell. */
import { activeWorkshopId } from "$lib/utils/workshopLocality";

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
  if (!cardId.trim()) return;
  if (ports.openWorkCard) return ports.openWorkCard(cardId);
  const scope = activeWorkshopId();
  const { workspace } = await import("$lib/stores/workspace.svelte");
  const { commandSpotlight } = await import("$lib/stores/commandSpotlight.svelte");
  if (activeWorkshopId() !== scope) return;
  commandSpotlight.closeSpotlight();
  await workspace.selectCard(cardId, { refresh: true });
}

/** New entry points use task terminology; native notification callers keep their API. */
export const openTaskDetails = openWorkCard;

export async function openVaultNote(notePath: string): Promise<void> {
  if (ports.openVaultNote) return ports.openVaultNote(notePath);
  const scope = activeWorkshopId();
  const { layout } = await import("$lib/runtime/layout.svelte");
  if (activeWorkshopId() !== scope) return;
  if (layout.isMobile) {
    const { vault } = await import("$lib/stores/vault.svelte");
    if (activeWorkshopId() !== scope) return;
    await vault.openNote(notePath);
    if (activeWorkshopId() !== scope) return;
    vault.enterPreviewMode();
    layout.setLibraryView("reader");
    layout.openNotes();
    return;
  }
  const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
  if (activeWorkshopId() !== scope) return;
  layout.navigateDesktop("library");
  await lmeWorkspace.openNote(notePath);
}

export async function openPeerThread(input: PeerThreadInput): Promise<void> {
  if (ports.openPeerThread) return ports.openPeerThread(input);
  const { setPendingPeerNavigation } = await import("$lib/peerNavigation");
  const { layout } = await import("$lib/runtime/layout.svelte");
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
  const { layout } = await import("$lib/runtime/layout.svelte");
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
  void import("$lib/runtime/layout.svelte").then(({ layout }) => {
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
