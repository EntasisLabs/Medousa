import { chat } from "$lib/stores/chat.svelte";
import { recurring } from "$lib/stores/recurring.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { ensureMobileDaemonUrl } from "$lib/daemonConnection";
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

function registerStreamListeners(unlisteners: Promise<() => void>[]) {
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
}

async function startWorkshopStreams(): Promise<void> {
  await stopWorkspaceStream();
  await startWorkspaceStream(workspace.revision || undefined);
  void recurring.refresh();
  void chat.refreshSessions();
  if (chat.messages.length === 0) {
    void chat.switchSession(chat.sessionId);
  }
}

async function loadWorkshopDefaults(): Promise<void> {
  try {
    await runtime.loadFromTuiDefaults();
    void runtime.refresh();
  } catch {
    // Local defaults are optional when offline.
  }
}

export async function reconnectWorkshop(
  onHealthChange: (health: DaemonHealth | null) => void,
): Promise<DaemonHealth> {
  await ensureMobileDaemonUrl();
  const health = await checkDaemonHealth();
  onHealthChange(health);

  await stopWorkspaceStream();
  stopInteractiveStream();

  if (health.ok) {
    await startWorkshopStreams();
  }

  return health;
}

/**
 * Shared daemon + SSE bootstrap for desktop and mobile shells.
 */
export function connectWorkshop(options: {
  onHealthChange: (health: DaemonHealth | null) => void;
}): () => void {
  settings.applyTheme();
  const unlisteners: Promise<() => void>[] = [];
  registerStreamListeners(unlisteners);

  void (async () => {
    try {
      options.onHealthChange(null);
      await ensureMobileDaemonUrl();
      const health = await checkDaemonHealth();
      options.onHealthChange(health);

      void loadWorkshopDefaults();

      if (health.ok) {
        await startWorkshopStreams();
      }
    } catch (err) {
      options.onHealthChange({
        ok: false,
        message: err instanceof Error ? err.message : String(err),
      });
    }
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
