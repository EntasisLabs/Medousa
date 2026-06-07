import { chat } from "$lib/stores/chat.svelte";
import { recurring } from "$lib/stores/recurring.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import {
  checkDaemonHealth,
  onInteractiveEvent,
  onInteractiveError,
  onWorkspaceEvent,
  onWorkspaceError,
  startWorkspaceStream,
  stopInteractiveStream,
  stopWorkspaceStream,
  type DaemonHealth,
} from "$lib/daemon";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { WorkspaceStreamEvent } from "$lib/types/workspace";

export type WorkshopConnection = {
  getHealth: () => DaemonHealth | null;
  refreshHealth: () => Promise<DaemonHealth | null>;
};

/**
 * Shared daemon + SSE bootstrap for desktop and mobile shells.
 */
export function connectWorkshop(options: {
  onHealthChange: (health: DaemonHealth | null) => void;
}): () => void {
  settings.applyTheme();
  let health: DaemonHealth | null = null;
  const unlisteners: Promise<() => void>[] = [];

  (async () => {
    health = await checkDaemonHealth();
    options.onHealthChange(health);

    await stopWorkspaceStream();
    await startWorkspaceStream(workspace.revision || undefined);
    await runtime.loadFromTuiDefaults();
    void runtime.refresh();
    void recurring.refresh();
    void chat.refreshSessions();
    if (chat.messages.length === 0) {
      void chat.switchSession(chat.sessionId);
    }

    unlisteners.push(
      onWorkspaceEvent<WorkspaceStreamEvent>((event) => {
        workspace.applyEvent(event);
        const kind = event.feed_event?.kind;
        if (kind === "vault_note_created" || kind === "vault_note_updated") {
          void vault.refreshNotes();
          if (
            vault.selectedPath &&
            event.feed_event?.summary.includes(vault.selectedPath)
          ) {
            void vault.openNote(vault.selectedPath);
          }
        }
      }),
    );
    unlisteners.push(onWorkspaceError((message) => workspace.setError(message)));
    unlisteners.push(
      onInteractiveEvent<InteractiveTurnStreamEvent>((event) => {
        chat.applyStreamEvent(event);
      }),
    );
    unlisteners.push(onInteractiveError((message) => chat.setError(message)));
  })();

  return () => {
    Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    stopWorkspaceStream();
    stopInteractiveStream();
  };
}

export async function refreshDaemonHealth(): Promise<DaemonHealth | null> {
  return checkDaemonHealth();
}
