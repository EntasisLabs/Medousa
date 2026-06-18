import { chat } from "$lib/stores/chat.svelte";
import { connection } from "$lib/stores/connection.svelte";
import { recurring } from "$lib/stores/recurring.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
import { voicePresets } from "$lib/stores/voicePresets.svelte";
import { userProfiles } from "$lib/stores/userProfiles.svelte";
import { ensureMobileDaemonUrl } from "$lib/daemonConnection";
import {
  budgetRequestIdFromStreamEvent,
  notifyBudgetApprovalRequired,
  notifyTurnTicketTerminal,
  notifyWorkerHandoff,
} from "$lib/notifications";
import { isWorkerHandoffStreamEvent, isRecoverableStreamError } from "$lib/utils/streamEvents";
import { isTauriMobilePlatform } from "$lib/platform";
import { sendPairingHeartbeat } from "$lib/utils/pairingClient";
import { haptic } from "$lib/haptics";
import {
  checkDaemonHealth,
  onInteractiveEvent,
  onInteractiveError,
  onWorkspaceEvent,
  onWorkspaceError,
  startWorkspaceStream,
  stopWorkspaceStream,
  type DaemonHealth,
} from "$lib/daemon";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { WorkspaceStreamEvent } from "$lib/types/workspace";

export type WorkshopConnection = {
  getHealth: () => DaemonHealth | null;
  refreshHealth: () => Promise<DaemonHealth | null>;
};

const MAX_STREAM_RECONNECT_DELAY_MS = 30_000;

let workshopTeardown = false;
let workspaceReconnectAttempt = 0;
let workspaceReconnectTimer: ReturnType<typeof setTimeout> | null = null;
let interactiveRecoverTimer: ReturnType<typeof setTimeout> | null = null;
let resumeWorkshopInFlight = false;
let lastResumeWorkshopAt = 0;
const RESUME_DEBOUNCE_MS = 3_000;

function cancelScheduledStreamRecovery() {
  if (workspaceReconnectTimer) {
    clearTimeout(workspaceReconnectTimer);
    workspaceReconnectTimer = null;
  }
  workspaceReconnectAttempt = 0;
  if (interactiveRecoverTimer) {
    clearTimeout(interactiveRecoverTimer);
    interactiveRecoverTimer = null;
  }
}

function scheduleWorkspaceStreamReconnect() {
  if (workshopTeardown || workspaceReconnectTimer) return;

  const delayMs = Math.min(
    1_000 * 2 ** workspaceReconnectAttempt,
    MAX_STREAM_RECONNECT_DELAY_MS,
  );
  workspaceReconnectAttempt += 1;

  workspaceReconnectTimer = setTimeout(() => {
    workspaceReconnectTimer = null;
    void recoverWorkspaceStream();
  }, delayMs);
}

async function recoverWorkspaceStream(): Promise<void> {
  if (workshopTeardown) return;

  try {
    const health = await checkDaemonHealth();
    connection.setHealth(health);
    if (!health.ok) {
      scheduleWorkspaceStreamReconnect();
      return;
    }

    await stopWorkspaceStream();
    await startWorkspaceStream(workspace.revision || undefined);
    workspaceReconnectAttempt = 0;
    void chat.tryReattachActiveTurn(workspace.cards);
  } catch {
    scheduleWorkspaceStreamReconnect();
  }
}

function scheduleInteractiveStreamRecover() {
  if (workshopTeardown || interactiveRecoverTimer) return;

  interactiveRecoverTimer = setTimeout(() => {
    interactiveRecoverTimer = null;
    void recoverInteractiveStreams();
  }, 500);
}

async function recoverInteractiveStreams(): Promise<void> {
  const needsStream = [...chat.turns.values()].some(
    (turn) =>
      !turn.terminal &&
      turn.mode === "interactive" &&
      turn.phase !== "worker_handoff" &&
      turn.phase !== "budget_blocked",
  );
  const attached = await chat.tryReattachActiveTurn(workspace.cards);
  if (!attached && needsStream) {
    chat.noteStreamFailure("Could not reattach to live turn", { recoverable: true });
  } else if (attached) {
    chat.streamError = null;
  }
}

/** Restart SSE pipes without a full settings/runtime reload. */
async function restartWorkshopStreamsLite(): Promise<void> {
  await stopWorkspaceStream();
  await startWorkspaceStream(workspace.revision || undefined);
  void chat.tryReattachActiveTurn(workspace.cards);
}

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
  unlisteners.push(
    onWorkspaceError((message) => {
      workspace.setError(message);
      scheduleWorkspaceStreamReconnect();
    }),
  );
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
  unlisteners.push(
    onInteractiveError((message) => {
      chat.noteStreamFailure(message, {
        recoverable: isRecoverableStreamError(message),
      });
      scheduleInteractiveStreamRecover();
    }),
  );
}

async function startWorkshopStreams(): Promise<void> {
  cancelScheduledStreamRecovery();
  await stopWorkspaceStream();
  await startWorkspaceStream(workspace.revision || undefined);
  void recurring.refresh();
  await Promise.all([
    chat.refreshSessions({ force: true }),
    chat.ensureSessionHydrated({ notice: false }),
  ]);
  void chat.tryReattachActiveTurn(workspace.cards);
  void chat.hydrateAskThreads(workspace.cards);
  void workspace.syncTurnWorkerCardsToChat();
}

async function loadWorkshopDefaults(connected: boolean): Promise<void> {
  try {
    await runtime.loadWorkshopRuntime({ connected });
    if (connected) {
      await workshopDefaults.load(true);
      await voicePresets.load(true);
      await userProfiles.load();
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
  const now = Date.now();
  if (resumeWorkshopInFlight || now - lastResumeWorkshopAt < RESUME_DEBOUNCE_MS) {
    return;
  }
  resumeWorkshopInFlight = true;
  lastResumeWorkshopAt = now;

  if (isTauriMobilePlatform()) {
    void sendPairingHeartbeat().catch(() => {});
  }

  let health: DaemonHealth;
  try {
    health = await checkDaemonHealth();
  } finally {
    resumeWorkshopInFlight = false;
  }
  connection.setHealth(health);
  onHealthChange(health);
  if (!health.ok) return;

  await Promise.all([
    chat.reconcileOnResume({ notice: false }, workspace.cards),
    chat.hydrateAskThreads(workspace.cards),
    workspace.reconcileCardsFromSnapshot(),
  ]);

  try {
    await restartWorkshopStreamsLite();
  } catch {
    scheduleWorkspaceStreamReconnect();
  }
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
  cancelScheduledStreamRecovery();
  await ensureMobileDaemonUrl();
  const health = await checkDaemonHealth();
  connection.setHealth(health);
  onHealthChange(health);

  await stopWorkspaceStream();
  await chat.stopOwnedInteractiveStreams();

  if (health.ok) {
    runtime.resetWorkshopRuntime();
    workshopDefaults.resetForReconnect();
    userProfiles.resetForReconnect();
    await runtime.loadWorkshopRuntime({ connected: true });
    await workshopDefaults.load(true);
    await userProfiles.load();
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
  workshopTeardown = false;
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
    workshopTeardown = true;
    cancelScheduledStreamRecovery();
    detachForeground();
    Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    void (async () => {
      await stopWorkspaceStream();
      await chat.stopOwnedInteractiveStreams();
    })();
  };
}

export async function refreshDaemonHealth(): Promise<DaemonHealth | null> {
  return checkDaemonHealth();
}
