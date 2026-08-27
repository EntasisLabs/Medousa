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
import { identity } from "$lib/stores/identity.svelte";
import { workshops } from "$lib/stores/workshops.svelte";
import { ensureMobileDaemonUrl } from "$lib/daemonConnection";
import {
  budgetRequestIdFromStreamEvent,
  notifyBudgetApprovalRequired,
  notifyTurnTicketTerminal,
  notifyWorkerHandoff,
} from "$lib/notifications";
import { isRecoverableStreamError } from "$lib/utils/streamEvents";
import {
  DEFAULT_INTERACTIVE_BACKOFF,
  DEFAULT_WORKSPACE_BACKOFF,
  ReconnectScheduler,
} from "$lib/stream/reconnect";
import { isTauriMobilePlatform } from "$lib/platform";
import { sendPairingHeartbeat } from "$lib/utils/pairingClient";
import { haptic } from "$lib/haptics";
import { ensureWorkshopEngineHealthy } from "$lib/utils/ensureWorkshopEngine";
import {
  checkDaemonHealth,
  getDaemonUrl,
  invalidateRouteCaches,
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
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
import type { WorkspaceStreamEvent } from "$lib/types/workspace";

export type WorkshopConnection = {
  getHealth: () => DaemonHealth | null;
  refreshHealth: () => Promise<DaemonHealth | null>;
};

export type WorkshopConnectMode = "full" | "observer";

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
let workshopTransitioning = false;
let workshopConnectMode: WorkshopConnectMode = "full";
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
  if (workshopTeardown || workshopConnectMode === "observer") return;
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
  if (workshopTeardown || workshopConnectMode === "observer") return;
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
  if (workshopTeardown || workshopConnectMode === "observer") return;
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
    chat.streamError = null;
    return;
  }
  // Daemon idle clears orphans inside tryReattach; only alarm when still live.
  if (needsStream && chat.hasLiveInteractiveTurn()) {
    chat.noteStreamFailure("Could not reattach to live turn", { recoverable: true });
    return;
  }
  chat.streamError = null;
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
      if (workshopTransitioning) return;
      environment.applyEvent(event);
    }),
  );
  unlisteners.push(
    onEnvironmentError((error) => {
      if (workshopTransitioning) return;
      environment.setError(error.message);
      scheduleEnvironmentStreamReconnect();
    }),
  );
  unlisteners.push(
    onWorkspaceEvent<WorkspaceStreamEvent>((event) => {
      if (workshopTransitioning) return;
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
    onWorkspaceError((error) => {
      if (workshopTransitioning) return;
      workspace.setError(error.message);
      scheduleWorkspaceStreamReconnect();
    }),
  );
  unlisteners.push(
    onInteractiveEvent<TurnStreamEnvelopeV3>((envelope) => {
      if (workshopTransitioning) return;
      const turnBefore = chat.turns.get(envelope.turn_id);
      chat.applyStreamEvent(envelope);
      if (!isTauriMobilePlatform()) return;

      if (envelope.event.type === "budget_approval_required") {
        const requestId = budgetRequestIdFromStreamEvent(envelope);
        if (requestId) {
          void notifyBudgetApprovalRequired(
            envelope.event.reason.split(".")[0]?.trim() || "Turn paused",
            requestId,
            envelope.event.reason,
          );
          haptic("warning");
        }
        return;
      }

      if (
        envelope.event.type === "worker_ack" &&
        envelope.event.ack_kind === "worker"
      ) {
        void notifyWorkerHandoff(envelope, turnBefore?.workspaceCardId);
        haptic("light");
        return;
      }

      if (envelope.event.type === "turn_completed") {
        void notifyTurnTicketTerminal(envelope, turnBefore?.workspaceCardId);
        haptic("success");
      }
    }),
  );
  unlisteners.push(
    onInteractiveError((error) => {
      if (workshopTransitioning) return;
      chat.noteStreamFailure(error.message, {
        recoverable: error.recoverable ?? isRecoverableStreamError(error.message),
      });
      scheduleInteractiveStreamRecover();
    }),
  );
}

/** Stop the selected daemon's effects and remove its in-memory projections. */
export async function prepareForWorkshopSwitch(): Promise<void> {
  workshopTransitioning = true;
  cancelScheduledStreamRecovery();
  connection.setHealth(null);
  await Promise.all([
    stopWorkspaceStream().catch(() => undefined),
    stopEnvironmentSync().catch(() => undefined),
    chat.stopOwnedInteractiveStreams().catch(() => undefined),
  ]);
  clearWorkshopState();
}

function clearWorkshopState(): void {
  chat.prepareForWorkshopSwitch();
  runtime.resetWorkshopRuntime();
  workshopDefaults.resetForReconnect();
  userProfiles.resetForReconnect();
  identity.clear();
  environment.resetForReconnect();
  vault.resetForWorkshopSwitch();
  workspace.resetForWorkshopSwitch();
  automations.resetForWorkshopSwitch();
  voicePresets.resetForWorkshopSwitch();
}

/** Bind client-only caches after Rust has committed the new active selection. */
export function activateWorkshopScope(workshopId: string): void {
  if (chat.workshopScopeId && chat.workshopScopeId !== workshopId) {
    clearWorkshopState();
  }
  chat.activateWorkshopScope(workshopId);
}

async function startWorkshopStreams(): Promise<void> {
  cancelScheduledStreamRecovery();
  await stopWorkspaceStream();
  await stopEnvironmentSync();
  await Promise.all([
    environment.load(),
    vault.refreshVaultRoots(),
    vault.refreshNotes(),
  ]);
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

async function bootstrapWorkshopObserver(): Promise<void> {
  await Promise.all([
    chat.refreshSessions({ force: true }),
    chat.sessionPristine
      ? Promise.resolve()
      : chat.reloadCurrentSession({ notice: false }),
  ]);
  await workspace.reconcileCardsFromSnapshot();
  await workspace.recoverPendingWorkerResults();
}

export async function resumeWorkshopObserver(
  onHealthChange: (health: DaemonHealth | null) => void,
): Promise<void> {
  const now = Date.now();
  if (resumeWorkshopInFlight || now - lastResumeWorkshopAt < RESUME_DEBOUNCE_MS) {
    return;
  }
  resumeWorkshopInFlight = true;
  lastResumeWorkshopAt = now;

  try {
    await invalidateRouteCaches().catch(() => {});
    // Observer does not own spawn — main window / connectWorkshop does.
    const health = await checkDaemonHealth();
    connection.setHealth(health);
    onHealthChange(health);
    if (!health.ok) return;

    await workspace.reconcileCardsFromSnapshot();
    await Promise.all([
      chat.reconcileOnResume({ notice: false }, workspace.cards),
      chat.hydrateAskThreads(workspace.cards),
    ]);
    await workspace.recoverPendingWorkerResults();
  } finally {
    resumeWorkshopInFlight = false;
  }
}

export function attachWorkshopObserverForegroundResume(
  onHealthChange: (health: DaemonHealth | null) => void,
): () => void {
  if (typeof document === "undefined") return () => {};

  const handler = () => {
    if (document.visibilityState !== "visible") return;
    void resumeWorkshopObserver(onHealthChange);
  };

  document.addEventListener("visibilitychange", handler);
  return () => document.removeEventListener("visibilitychange", handler);
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

  try {
    if (isTauriMobilePlatform()) {
      void sendPairingHeartbeat().catch(() => {});
    }

    // A network handoff (WiFi↔LTE, Mac sleep/DHCP) may have happened while we were
    // backgrounded. Flush both route caches so the health probe below re-picks
    // LAN vs Iroh instead of riding a stale cached route for the rest of its TTL.
    await invalidateRouteCaches().catch(() => {});

    // P0.3 — if the sidecar died over sleep, spawn again before giving up.
    const health = await ensureWorkshopEngineHealthy({ allowSpawn: true });
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
  } finally {
    resumeWorkshopInFlight = false;
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
  try {
    cancelScheduledStreamRecovery();
    await Promise.all([
      stopWorkspaceStream().catch(() => undefined),
      stopEnvironmentSync().catch(() => undefined),
      chat.stopOwnedInteractiveStreams().catch(() => undefined),
    ]);
    await ensureMobileDaemonUrl();
    await invalidateRouteCaches().catch(() => {});
    const health = await ensureWorkshopEngineHealthy({ allowSpawn: true });
    connection.setHealth(health);
    onHealthChange(health);

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
      workshopTransitioning = false;
      await startWorkshopStreams();
      await workshops.restoreLastSession();
    }

    return health;
  } finally {
    workshopTransitioning = false;
  }
}

/**
 * Shared daemon + SSE bootstrap for desktop and mobile shells.
 *
 * `observer` mode (pop-out chat): listens to broadcast SSE events without
 * starting or tearing down global stream pipes owned by the main window.
 */
export function connectWorkshop(options: {
  onHealthChange: (health: DaemonHealth | null) => void;
  mode?: WorkshopConnectMode;
}): () => void {
  const mode = options.mode ?? "full";
  workshopConnectMode = mode;
  workshopTeardown = false;
  chat.setStreamRole(mode === "observer" ? "observer" : "owner");
  settings.applyTheme();
  const unlisteners: Promise<() => void>[] = [];
  registerStreamListeners(unlisteners);

  const detachForeground =
    mode === "full"
      ? attachWorkshopForegroundResume(options.onHealthChange)
      : attachWorkshopObserverForegroundResume(options.onHealthChange);

  void (async () => {
    let health: DaemonHealth;
    try {
      await workshops.load();
      connection.setHealth(null);
      options.onHealthChange(null);
      await ensureMobileDaemonUrl();
      // P0.1 — day-2+ launch: spawn local engine when health is down (wizard warm
      // only runs while the first-run sheet is visible).
      health = await ensureWorkshopEngineHealthy({
        allowSpawn: mode === "full",
      });
      connection.setHealth(health);
      options.onHealthChange(health);
    } catch (err) {
      const failed = {
        ok: false,
        message: err instanceof Error ? err.message : String(err),
      };
      connection.setHealth(failed);
      options.onHealthChange(failed);
      return;
    }

    void loadWorkshopDefaults(health.ok);

    try {
      if (health.ok) {
        if (mode === "full") {
          await startWorkshopStreams();
          await workshops.restoreLastSession();
          // Shell tabs may mount before the daemon finishes starting. Re-read
          // the selected conversation once the engine is actually ready; only
          // explicit New Chat sessions are allowed to remain pristine/blank.
          if (!chat.sessionPristine) {
            await chat.reloadCurrentSession({ notice: false });
          }
          void registerBrowserHostClient(health);
        } else {
          await bootstrapWorkshopObserver();
        }
      }
      workshops.applyThemeForActiveWorkshop();
    } catch (err) {
      // Projection/bootstrap failures do not make a healthy daemon offline.
      // Keep the composer usable; existing stream errors own their recovery.
      chat.noteResumeFailure(err);
    }
  })();

  return () => {
    workshopTeardown = true;
    detachForeground();
    Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    if (mode === "full") {
      cancelScheduledStreamRecovery();
      workspaceReconnect.teardown();
      interactiveReconnect.teardown();
      void (async () => {
        await stopWorkspaceStream();
        await stopEnvironmentSync();
        await chat.stopOwnedInteractiveStreams();
      })();
    }
  };
}

export async function refreshDaemonHealth(): Promise<DaemonHealth | null> {
  return checkDaemonHealth();
}
