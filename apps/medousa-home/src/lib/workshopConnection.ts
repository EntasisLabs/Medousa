import { chat } from "$lib/stores/chat.svelte";
import { connection } from "$lib/stores/connection.svelte";
import { automations } from "$lib/stores/automations.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
import { voicePresets } from "$lib/stores/voicePresets.svelte";
import { userProfiles } from "$lib/stores/userProfiles.svelte";
import { workshops } from "$lib/stores/workshops.svelte";
import { ensureMobileDaemonUrl } from "$lib/daemonConnection";
import {
  budgetRequestIdFromStreamEvent,
  notifyBudgetApprovalRequired,
  notifyTurnTicketTerminal,
  notifyWorkerHandoff,
} from "$lib/notifications";
import { isWorkerHandoffStreamEvent, isRecoverableStreamError } from "$lib/utils/streamEvents";
import {
  DEFAULT_INTERACTIVE_BACKOFF,
  DEFAULT_WORKSPACE_BACKOFF,
  ReconnectScheduler,
} from "$lib/stream/reconnect";
import { isTauriMobilePlatform } from "$lib/platform";
import { sendPairingHeartbeat } from "$lib/utils/pairingClient";
import { haptic } from "$lib/haptics";
import {
  checkDaemonHealth,
  getDaemonUrl,
  onEnvironmentError,
  onEnvironmentEvent,
  onInteractiveEvent,
  onInteractiveError,
  onWorkspaceEvent,
  onWorkspaceError,
  registerBrowserClient,
  startEnvironmentStream,
  stopEnvironmentStream,
  startWorkspaceStream,
  stopWorkspaceStream,
  type DaemonHealth,
} from "$lib/daemon";
import {
  environment,
  startEnvironmentSync,
  stopEnvironmentSync,
} from "$lib/stores/environment.svelte";
import type { EnvironmentStreamEvent } from "$lib/types/environment";
import { homeChannelSurface } from "$lib/platform";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { WorkspaceStreamEvent } from "$lib/types/workspace";

export type WorkshopConnection = {
  getHealth: () => DaemonHealth | null;
  refreshHealth: () => Promise<DaemonHealth | null>;
};

async function registerBrowserHostClient(health: DaemonHealth): Promise<void> {
  if (!health.ok) return;
  try {
    const daemonUrl = await getDaemonUrl();
    await registerBrowserClient(daemonUrl, homeChannelSurface());
  } catch {
    // Browser host registration is best-effort on connect.
  }
}

let workshopTeardown = false;
const workspaceReconnect = new ReconnectScheduler({
  policy: DEFAULT_WORKSPACE_BACKOFF,
});
const interactiveReconnect = new ReconnectScheduler({
  policy: DEFAULT_INTERACTIVE_BACKOFF,
});
let resumeWorkshopInFlight = false;
let lastResumeWorkshopAt = 0;
const RESUME_DEBOUNCE_MS = 3_000;

function cancelScheduledStreamRecovery() {
  workspaceReconnect.cancel();
  interactiveReconnect.cancel();
}

function scheduleEnvironmentStreamReconnect() {
  if (workshopTeardown) return;
  workspaceReconnect.schedule(() => recoverEnvironmentStream());
}

async function recoverEnvironmentStream(): Promise<void> {
  if (workshopTeardown) return;
  try {
    const health = await checkDaemonHealth();
    connection.setHealth(health);
    if (!health.ok) {
      scheduleEnvironmentStreamReconnect();
      return;
    }
    await stopEnvironmentSync();
    await environment.load();
    await startEnvironmentSync();
  } catch {
    scheduleEnvironmentStreamReconnect();
  }
}

function scheduleWorkspaceStreamReconnect() {
  if (workshopTeardown) return;
  workspaceReconnect.schedule(() => recoverWorkspaceStream());
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
    workspaceReconnect.noteSuccess();
    await workspace.recoverPendingWorkerResults();
    void chat.tryReattachActiveTurn(workspace.cards);
  } catch {
    scheduleWorkspaceStreamReconnect();
  }
}

function scheduleInteractiveStreamRecover() {
  if (workshopTeardown) return;
  interactiveReconnect.schedule(() => recoverInteractiveStreams());
}

async function recoverInteractiveStreams(): Promise<void> {
  const needsStream = [...chat.turns.values()].some(
    (turn) =>
      !turn.terminal &&
      turn.mode === "interactive" &&
      turn.phase !== "worker_handoff" &&
      turn.phase !== "workshop_handoff" &&
      turn.phase !== "budget_blocked",
  );
  const attached = await chat.tryReattachActiveTurn(workspace.cards);
  if (attached) {
    interactiveReconnect.noteSuccess();
  }
  if (!attached && needsStream) {
    chat.noteStreamFailure("Could not reattach to live turn", { recoverable: true });
  } else if (attached || !needsStream) {
    chat.streamError = null;
  }
}

/** Restart SSE pipes without a full settings/runtime reload. */
async function restartWorkshopStreamsLite(): Promise<void> {
  await stopWorkspaceStream();
  await stopEnvironmentSync();
  await startWorkspaceStream(workspace.revision || undefined);
  await startEnvironmentSync();
  void chat.tryReattachActiveTurn(workspace.cards);
}

function registerStreamListeners(unlisteners: Promise<() => void>[]) {
  unlisteners.push(
    onEnvironmentEvent<EnvironmentStreamEvent>((event) => {
      environment.applyEvent(event);
    }),
  );
  unlisteners.push(
    onEnvironmentError((message) => {
      environment.setError(message);
      scheduleEnvironmentStreamReconnect();
    }),
  );
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
  await stopEnvironmentSync();
  await environment.load();
  await startWorkspaceStream(workspace.revision || undefined);
  await startEnvironmentSync();
  void automations.refresh();
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
    if (connected) {
      await workshopDefaults.load(true);
      if (workshopDefaults.loaded) {
        runtime.applyFromWorkshopDraft(workshopDefaults.draft);
      }
      await voicePresets.load(true);
      await userProfiles.load();
      await settings.hydrateWorkRetentionFromDaemon();
      void runtime.refresh();
    } else {
      await runtime.loadWorkshopRuntime({ connected: false });
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

  void registerBrowserHostClient(health);

  // Cards first so handoff synthesis recovery has an authoritative board.
  await workspace.reconcileCardsFromSnapshot();

  await Promise.all([
    chat.reconcileOnResume({ notice: false }, workspace.cards),
    chat.hydrateAskThreads(workspace.cards),
    userProfiles.syncOnResume(health),
    // If the WebView was evicted while backgrounded, the open note's path
    // survives but its body does not. Re-fetch so the reader is not blank.
    vault.selectedPath && !vault.content
      ? vault.reloadFromServer()
      : Promise.resolve(),
  ]);

  // History merge may link workers missed while SSE was detached.
  await workspace.recoverPendingWorkerResults();

  try {
    await restartWorkshopStreamsLite();
  } catch {
    scheduleWorkspaceStreamReconnect();
  }

  // Glance surfaces (Live Activity / home widget) need a forced quiet/working sync
  // after cards refresh — otherwise they stay stuck on the pre-background snapshot.
  if (isTauriMobilePlatform()) {
    try {
      const { isTauriIos } = await import("$lib/platform");
      if (isTauriIos()) {
        const { bumpLiveActivitySync, syncLiveActivity, buildLiveActivityPayload } =
          await import("$lib/liveActivity");
        const { bumpHomeWidgetSync, syncHomeWidget } = await import("$lib/homeWidget");
        const payload = buildLiveActivityPayload({
          health,
          cards: workspace.cards,
          blocked: workspace.blockedCount(),
          inMotion: workspace.inMotionCount(),
          primaryCard: workspace.primaryInMotionCard(),
          workshopName: workshops.activeLabel,
        });
        bumpLiveActivitySync();
        bumpHomeWidgetSync();
        if (settings.liveActivityEnabled) {
          void syncLiveActivity(payload, { force: true });
        }
        void syncHomeWidget(payload, { force: true });
      }
    } catch {
      // Glance sync is best-effort on resume.
    }
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
    environment.resetForReconnect();
    vault.resetForWorkshopSwitch();
    await workshopDefaults.load(true);
    if (workshopDefaults.loaded) {
      runtime.applyFromWorkshopDraft(workshopDefaults.draft);
    }
    await userProfiles.load();
    await settings.hydrateWorkRetentionFromDaemon();
    await startWorkshopStreams();
    await workshops.restoreLastSession();
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
        await workshops.restoreLastSession();
        void registerBrowserHostClient(health);
      }
      workshops.applyThemeForActiveWorkshop();
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
    workspaceReconnect.teardown();
    interactiveReconnect.teardown();
    detachForeground();
    Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    void (async () => {
      await stopWorkspaceStream();
      await stopEnvironmentSync();
      await chat.stopOwnedInteractiveStreams();
    })();
  };
}

export async function refreshDaemonHealth(): Promise<DaemonHealth | null> {
  return checkDaemonHealth();
}
