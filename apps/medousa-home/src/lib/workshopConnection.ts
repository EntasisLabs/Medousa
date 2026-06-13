import { chat } from "$lib/stores/chat.svelte";
import { connection } from "$lib/stores/connection.svelte";
import { recurring } from "$lib/stores/recurring.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
import { ensureMobileDaemonUrl } from "$lib/daemonConnection";
import {
  budgetRequestIdFromStreamEvent,
  notifyBudgetApprovalRequired,
  notifyTurnTicketTerminal,
  notifyWorkerHandoff,
} from "$lib/notifications";
import { isWorkerHandoffStreamEvent } from "$lib/utils/streamEvents";
import { isTauriMobilePlatform } from "$lib/platform";
import { haptic } from "$lib/haptics";
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
        if (event.feed_event) {
          vault.noteFromFeedEvent(event.feed_event);
        } else {
          vault.scheduleNotesRefresh();
        }
      }
    }),
  );
  unlisteners.push(onWorkspaceError((message) => workspace.setError(message)));
  unlisteners.push(
    onInteractiveEvent<InteractiveTurnStreamEvent>((event) => {
      const turnBefore = chat.turns.get(event.turn_id);
      chat.applyStreamEvent(event);
      if (!isTauriMobilePlatform()) return;

      if (event.event_type === "budget_approval") {
        const requestId = budgetRequestIdFromStreamEvent(event);
        if (requestId) {
          void notifyBudgetApprovalRequired(
            event.message.split(".")[0]?.trim() || "Turn paused",
            requestId,
            event.message,
          );
          haptic("warning");
        }
        return;
      }

      if (isWorkerHandoffStreamEvent(event)) {
        void notifyWorkerHandoff(event, turnBefore?.workspaceCardId);
        haptic("light");
        return;
      }

      if (event.terminal) {
        void notifyTurnTicketTerminal(event, turnBefore?.workspaceCardId);
        haptic("success");
      }
    }),
  );
  unlisteners.push(onInteractiveError((message) => chat.setError(message)));
}

async function startWorkshopStreams(): Promise<void> {
  await stopWorkspaceStream();
  await startWorkspaceStream(workspace.revision || undefined);
  void recurring.refresh();
  await Promise.all([
    chat.refreshSessions({ force: true }),
    chat.ensureSessionHydrated({ notice: true }),
  ]);
  void chat.tryReattachActiveTurn();
  void chat.hydrateAskThreads(workspace.cards);
  void chat.tryReattachAskTurns(workspace.cards);
  void workspace.syncTurnWorkerCardsToChat();
}

async function loadWorkshopDefaults(connected: boolean): Promise<void> {
  try {
    await runtime.loadWorkshopRuntime({ connected });
    if (connected) {
      await workshopDefaults.load(true);
      await settings.hydrateWorkRetentionFromDaemon();
      void runtime.refresh();
    }
  } catch {
    // Workshop defaults are optional when offline.
  }
}

export async function resumeWorkshop(
  onHealthChange: (health: DaemonHealth | null) => void,
): Promise<void> {
  const health = await checkDaemonHealth();
  connection.setHealth(health);
  onHealthChange(health);
  if (!health.ok) return;

  await Promise.all([
    chat.reconcileOnResume({ notice: false }),
    chat.hydrateAskThreads(workspace.cards),
    chat.tryReattachAskTurns(workspace.cards),
  ]);
}

export function attachWorkshopForegroundResume(
  onHealthChange: (health: DaemonHealth | null) => void,
): () => void {
  if (typeof document === "undefined") return () => {};

  const handler = () => {
    if (document.visibilityState !== "visible") return;
    void resumeWorkshop(onHealthChange);
  };

  document.addEventListener("visibilitychange", handler);
  return () => document.removeEventListener("visibilitychange", handler);
}

export async function reconnectWorkshop(
  onHealthChange: (health: DaemonHealth | null) => void,
): Promise<DaemonHealth> {
  await ensureMobileDaemonUrl();
  const health = await checkDaemonHealth();
  connection.setHealth(health);
  onHealthChange(health);

  await stopWorkspaceStream();
  stopInteractiveStream();

  if (health.ok) {
    runtime.resetWorkshopRuntime();
    workshopDefaults.resetForReconnect();
    await runtime.loadWorkshopRuntime({ connected: true });
    await workshopDefaults.load(true);
    await settings.hydrateWorkRetentionFromDaemon();
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

  const detachForeground = attachWorkshopForegroundResume(options.onHealthChange);

  void (async () => {
    try {
      connection.setHealth(null);
      options.onHealthChange(null);
      await ensureMobileDaemonUrl();
      const health = await checkDaemonHealth();
      connection.setHealth(health);
      options.onHealthChange(health);

      void loadWorkshopDefaults(health.ok);

      if (health.ok) {
        await startWorkshopStreams();
      }
    } catch (err) {
      const failed = {
        ok: false,
        message: err instanceof Error ? err.message : String(err),
      };
      connection.setHealth(failed);
      options.onHealthChange(failed);
    }
  })();

  return () => {
    detachForeground();
    Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    stopWorkspaceStream();
    stopInteractiveStream();
  };
}

export async function refreshDaemonHealth(): Promise<DaemonHealth | null> {
  return checkDaemonHealth();
}
