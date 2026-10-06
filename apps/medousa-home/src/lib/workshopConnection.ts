import { chat } from "$lib/stores/chat.svelte";
import { bots } from "$lib/stores/bots.svelte";
import { executionTargets } from "$lib/stores/executionTargets.svelte";
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
import { isRecoverableStreamError } from "$lib/utils/streamEvents";
import {
  DEFAULT_INTERACTIVE_BACKOFF,
  DEFAULT_WORKSPACE_BACKOFF,
  ReconnectScheduler,
} from "$lib/stream/reconnect";
import { isBrowserWorkshop, isTauriMobilePlatform } from "$lib/platform";
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
import { layout } from "$lib/runtime/layout.svelte";
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
import type { WorkspaceStreamEvent } from "$lib/types/workspace";

export type WorkshopConnection = {
  getHealth: () => DaemonHealth | null;
  refreshHealth: () => Promise<DaemonHealth | null>;
};

export type WorkshopConnectMode = "full" | "observer";

async function registerBrowserHostClient(health: DaemonHealth): Promise<void> {
  if (!health.ok || isBrowserWorkshop()) return;
  try {
    const daemonUrl = await getDaemonUrl();
    await registerBrowserClient(daemonUrl, homeChannelSurface());
  } catch {
    // Browser host registration is best-effort on connect.
  }
}

let workshopRecoveryGeneration = 0;
let workshopTeardown = false;
let workshopTransitioning = false;
let workshopConnectMode: WorkshopConnectMode = "full";
let connectionReconnect = new ReconnectScheduler({ policy: DEFAULT_WORKSPACE_BACKOFF });
let environmentReconnect = new ReconnectScheduler({ policy: DEFAULT_WORKSPACE_BACKOFF });
let workspaceReconnect = new ReconnectScheduler({
  policy: DEFAULT_WORKSPACE_BACKOFF,
});
let interactiveReconnect = new ReconnectScheduler({
  policy: DEFAULT_INTERACTIVE_BACKOFF,
});
let resumeWorkshopInFlight = false;
let workshopRefreshTask: Promise<DaemonHealth> | null = null;
let lastResumeWorkshopAt = 0;
let awaitingWorkspaceRecoverySnapshot = false;
const RESUME_DEBOUNCE_MS = 3_000;
const TRUST_HEARTBEAT_INTERVAL_MS = 6 * 60 * 60 * 1_000;
const BROWSER_CLIENT_HEARTBEAT_INTERVAL_MS = 45_000;

function cancelScheduledStreamRecovery() {
  workshopRecoveryGeneration += 1;
  connectionReconnect.cancel();
  environmentReconnect.cancel();
  workspaceReconnect.cancel();
  interactiveReconnect.cancel();
  awaitingWorkspaceRecoverySnapshot = false;
}

function recoveryIsCurrent(generation: number): boolean {
  return generation === workshopRecoveryGeneration && !workshopTeardown && !workshopTransitioning;
}

/** Startup failures have no SSE pipe yet, so they need their own recovery owner. */
function scheduleWorkshopConnectionRecovery(
  onHealthChange: (health: DaemonHealth | null) => void,
) {
  if (workshopTeardown || workshopTransitioning) return;
  const generation = workshopRecoveryGeneration;
  const workshopId = workshops.activeWorkshop?.id;
  connectionReconnect.schedule(async () => {
    if (!recoveryIsCurrent(generation) || document.visibilityState === "hidden") return;
    try {
      const health = workshopConnectMode === "observer"
        ? await checkDaemonHealth()
        : await refreshWorkshopConnection(onHealthChange);
      if (workshopTeardown || workshopTransitioning || workshops.activeWorkshop?.id !== workshopId) return;
      if (workshopConnectMode === "observer") {
        if (!recoveryIsCurrent(generation)) return;
        connection.setHealth(health);
        onHealthChange(health);
        if (health.ok) await bootstrapWorkshopObserver();
      }
      if (!health.ok) {
        if (recoveryIsCurrent(generation)) scheduleWorkshopConnectionRecovery(onHealthChange);
        return;
      }
      connectionReconnect.noteSuccess();
      await loadWorkshopDefaults(true);
    } catch (error) {
      if (recoveryIsCurrent(generation)) {
        chat.noteResumeFailure(error);
        scheduleWorkshopConnectionRecovery(onHealthChange);
      }
    }
  });
}

function scheduleEnvironmentStreamReconnect() {
  if (workshopTeardown || workshopTransitioning || workshopConnectMode === "observer") return;
  environmentReconnect.schedule(() => recoverEnvironmentStream());
}

async function recoverEnvironmentStream(): Promise<void> {
  const generation = workshopRecoveryGeneration;
  if (!recoveryIsCurrent(generation) || document.visibilityState === "hidden") return;
  try {
    // The stream sends a newer spec snapshot from its revision cursor. A health
    // preflight and full environment load add traffic and can replace loaded UI
    // with defaults when the network is slow.
    await startEnvironmentSync();
    if (!recoveryIsCurrent(generation)) return;
  } catch {
    if (!recoveryIsCurrent(generation)) return;
    scheduleEnvironmentStreamReconnect();
  }
}

function scheduleWorkspaceStreamReconnect() {
  if (workshopTeardown || workshopTransitioning || workshopConnectMode === "observer") return;
  workspaceReconnect.schedule(() => recoverWorkspaceStream());
}

async function recoverWorkspaceStream(): Promise<void> {
  const generation = workshopRecoveryGeneration;
  if (!recoveryIsCurrent(generation) || document.visibilityState === "hidden") return;

  try {
    await stopWorkspaceStream();
    if (!recoveryIsCurrent(generation)) return;
    awaitingWorkspaceRecoverySnapshot = true;
    await startWorkspaceStream(workspace.revision || undefined);
    // Native start returns after spawning the connection task, not after the
    // handshake. Reset backoff and reconcile workers only on real stream data.
  } catch {
    if (!recoveryIsCurrent(generation)) return;
    scheduleWorkspaceStreamReconnect();
  }
}

function scheduleInteractiveStreamRecover() {
  if (workshopTeardown || workshopTransitioning || workshopConnectMode === "observer") return;
  const generation = workshopRecoveryGeneration;
  interactiveReconnect.schedule(async () => {
    if (!recoveryIsCurrent(generation)) return;
    try {
      await recoverInteractiveStreams();
    } catch {
      if (recoveryIsCurrent(generation)) scheduleInteractiveStreamRecover();
    }
  });
}

async function recoverInteractiveStreams(): Promise<void> {
  const generation = workshopRecoveryGeneration;
  if (!recoveryIsCurrent(generation)) return;
  const needsStream = [...chat.turns.values()].some(
    (turn) =>
      !turn.terminal &&
      turn.mode === "interactive" &&
      turn.phase !== "worker_handoff" &&
      turn.phase !== "workshop_handoff" &&
      turn.phase !== "budget_blocked",
  );
  const attached = await chat.tryReattachActiveTurn(workspace.cards);
  if (!recoveryIsCurrent(generation)) return;
  if (attached) {
    interactiveReconnect.noteSuccess();
    chat.streamError = null;
    return;
  }
  // Daemon idle clears orphans inside tryReattach; only alarm when still live.
  if (needsStream && chat.hasLiveInteractiveTurn()) {
    chat.noteStreamFailure("Could not reattach to live turn", { recoverable: true });
    scheduleInteractiveStreamRecover();
    return;
  }
  chat.streamError = null;
}

/** Restart SSE pipes without a full settings/runtime reload. */
async function restartWorkshopStreamsLite(): Promise<void> {
  const generation = workshopRecoveryGeneration;
  await stopWorkspaceStream();
  if (!recoveryIsCurrent(generation)) return;
  await stopEnvironmentSync();
  if (!recoveryIsCurrent(generation)) return;
  await startWorkspaceStream(workspace.revision || undefined);
  if (!recoveryIsCurrent(generation)) return;
  await startEnvironmentSync();
  if (!recoveryIsCurrent(generation)) return;
  void chat.tryReattachActiveTurn(workspace.cards).catch((error) => {
    if (recoveryIsCurrent(generation)) chat.noteResumeFailure(error);
  });
}

function registerStreamListeners(unlisteners: Promise<() => void>[]) {
  unlisteners.push(
    onEnvironmentEvent<EnvironmentStreamEvent>((event) => {
      if (workshopTransitioning || workshopTeardown) return;
      connection.noteTraffic();
      connectionReconnect.noteSuccess();
      environmentReconnect.noteSuccess();
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
      if (workshopTransitioning || workshopTeardown) return;
      connection.noteTraffic();
      connectionReconnect.noteSuccess();
      workspaceReconnect.noteSuccess();
      workspace.applyEvent(event);
      if (awaitingWorkspaceRecoverySnapshot && event.stream_event_type === "snapshot") {
        awaitingWorkspaceRecoverySnapshot = false;
        void workspace.recoverPendingWorkerResults().catch((error) => chat.noteResumeFailure(error));
        void chat.tryReattachActiveTurn(workspace.cards);
      }
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
      if (workshopTransitioning || workshopTeardown) return;
      connection.noteTraffic();
      connectionReconnect.noteSuccess();
      chat.applyStreamEvent(envelope);
      if (!isTauriMobilePlatform()) return;

      if (envelope.event.type === "budget_approval_required") {
        haptic("warning");
        return;
      }

      if (
        envelope.event.type === "turn_completed" &&
        envelope.event.outcome === "completed"
      ) {
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
  bots.resetForWorkshopSwitch();
  executionTargets.resetForWorkshopSwitch();
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
  bots.activateWorkshopScope(workshopId);
  executionTargets.activateWorkshopScope(workshopId);
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
  void executionTargets.refresh({ force: true }).catch(() => undefined);
  await Promise.all([
    chat.refreshSessions({ force: true }),
    chat.ensureSessionHydrated({ notice: false }),
    bots.refresh({ force: true }).catch(() => undefined),
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
  const generation = workshopRecoveryGeneration;

  try {
    await invalidateRouteCaches().catch(() => {});
    void sendPairingHeartbeat().catch(() => {});
    // Observer does not own spawn — main window / connectWorkshop does.
    const probeTrafficRevision = connection.trafficRevision;
    const health = await checkDaemonHealth();
    if (!recoveryIsCurrent(generation)) return;
    connection.setHealth(health, probeTrafficRevision);
    onHealthChange(health);
    if (!health.ok) {
      scheduleWorkshopConnectionRecovery(onHealthChange);
      return;
    }

    await workspace.reconcileCardsFromSnapshot();
    await Promise.all([
      chat.reconcileOnResume({ notice: false }, workspace.cards),
      chat.hydrateAskThreads(workspace.cards),
    ]);
    await workspace.recoverPendingWorkerResults();
  } catch (error) {
    if (recoveryIsCurrent(generation)) {
      chat.noteResumeFailure(error);
      scheduleWorkshopConnectionRecovery(onHealthChange);
    }
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
  if (workshopRefreshTask || resumeWorkshopInFlight || now - lastResumeWorkshopAt < RESUME_DEBOUNCE_MS) {
    return;
  }
  resumeWorkshopInFlight = true;
  lastResumeWorkshopAt = now;
  const generation = workshopRecoveryGeneration;

  try {
    await invalidateRouteCaches().catch(() => {});
    if (!recoveryIsCurrent(generation)) return;
    void sendPairingHeartbeat().catch(() => {});
    const remote = workshops.activeWorkshop?.kind !== "local";
    if (remote) {
      try { await restartWorkshopStreamsLite(); }
      catch { scheduleWorkspaceStreamReconnect(); scheduleEnvironmentStreamReconnect(); }
    }
    if (!recoveryIsCurrent(generation)) return;
    const probeTrafficRevision = connection.trafficRevision;
    const health = await ensureWorkshopEngineHealthy({ allowSpawn: !remote });
    if (!recoveryIsCurrent(generation)) return;
    connection.setHealth(health, probeTrafficRevision);
    onHealthChange(health);
    if (!health.ok) {
      scheduleWorkshopConnectionRecovery(onHealthChange);
      return;
    }

    void registerBrowserHostClient(health);

    if (!remote) {
      try { await restartWorkshopStreamsLite(); }
      catch { scheduleWorkspaceStreamReconnect(); scheduleEnvironmentStreamReconnect(); }
    }
    await workspace.reconcileCardsFromSnapshot();
    if (!recoveryIsCurrent(generation)) return;

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
  } catch (error) {
    if (recoveryIsCurrent(generation)) {
      chat.noteResumeFailure(error);
      scheduleWorkshopConnectionRecovery(onHealthChange);
    }
  } finally {
    resumeWorkshopInFlight = false;
  }
}

/** Refresh the same authority without clearing drafts, open notes, tabs, or turns. */
export async function refreshWorkshopConnection(
  onHealthChange: (health: DaemonHealth | null) => void,
): Promise<DaemonHealth> {
  if (workshopRefreshTask) {
    const health = await workshopRefreshTask;
    onHealthChange(health);
    return health;
  }
  if (workshopTransitioning || workshopTeardown) {
    throw new Error("The workshop connection is changing. Try again in a moment.");
  }
  workshopRefreshTask = refreshCurrentWorkshop(onHealthChange);
  try {
    return await workshopRefreshTask;
  } finally {
    workshopRefreshTask = null;
  }
}

async function refreshCurrentWorkshop(
  onHealthChange: (health: DaemonHealth | null) => void,
): Promise<DaemonHealth> {
  const generation = workshopRecoveryGeneration;
  connection.setRecovering(true);
  try {
    await ensureMobileDaemonUrl();
    await invalidateRouteCaches();
    // Registration is best-effort. The stream transport renews due sessions.
    void sendPairingHeartbeat().catch(() => {});
    const probeTrafficRevision = connection.trafficRevision;
    const health = await ensureWorkshopEngineHealthy({
      allowSpawn: workshops.activeWorkshop?.kind === "local",
    });
    if (!recoveryIsCurrent(generation)) {
      throw new Error("The workshop connection changed while refreshing.");
    }
    connection.setHealth(health, probeTrafficRevision);
    onHealthChange(health);
    if (!health.ok) {
      scheduleWorkshopConnectionRecovery(onHealthChange);
      return health;
    }

    cancelScheduledStreamRecovery();
    await Promise.all([
      stopWorkspaceStream(), stopEnvironmentSync(), chat.stopOwnedInteractiveStreams(),
    ]);
    const refreshGeneration = workshopRecoveryGeneration;
    await restartWorkshopStreamsLite();
    if (!recoveryIsCurrent(refreshGeneration)) throw new Error("The workshop connection changed while refreshing.");
    await workspace.reconcileCardsFromSnapshot();
    if (!recoveryIsCurrent(refreshGeneration)) throw new Error("The workshop connection changed while refreshing.");
    await Promise.all([
      environment.load(), vault.refreshVaultRoots(), vault.refreshNotes(),
      chat.refreshSessions({ force: true }),
      chat.reconcileOnResume({ notice: false }, workspace.cards),
      chat.hydrateAskThreads(workspace.cards),
      userProfiles.syncOnResume(health),
      executionTargets.refresh({ force: true }), bots.refresh({ force: true }),
    ]);
    if (!recoveryIsCurrent(refreshGeneration)) throw new Error("The workshop connection changed while refreshing.");
    await workspace.recoverPendingWorkerResults();
    void registerBrowserHostClient(health);
    return health;
  } catch (error) {
    scheduleWorkshopConnectionRecovery(onHealthChange);
    scheduleWorkspaceStreamReconnect();
    scheduleEnvironmentStreamReconnect();
    scheduleInteractiveStreamRecover();
    throw error;
  } finally {
    connection.setRecovering(false);
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
    if (connection.offline) scheduleWorkshopConnectionRecovery(onHealthChange);
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
  connectionReconnect.teardown();
  connectionReconnect = new ReconnectScheduler({ policy: DEFAULT_WORKSPACE_BACKOFF });
  cancelScheduledStreamRecovery();
  environmentReconnect.teardown();
  workspaceReconnect.teardown();
  interactiveReconnect.teardown();
  environmentReconnect = new ReconnectScheduler({ policy: DEFAULT_WORKSPACE_BACKOFF });
  workspaceReconnect = new ReconnectScheduler({ policy: DEFAULT_WORKSPACE_BACKOFF });
  interactiveReconnect = new ReconnectScheduler({ policy: DEFAULT_INTERACTIVE_BACKOFF });
  const generation = workshopRecoveryGeneration;
  chat.setStreamRole(mode === "observer" ? "observer" : "owner");
  settings.applyTheme();
  const unlisteners: Promise<() => void>[] = [];
  registerStreamListeners(unlisteners);

  const detachForeground =
    mode === "full"
      ? attachWorkshopForegroundResume(options.onHealthChange)
      : attachWorkshopObserverForegroundResume(options.onHealthChange);
  const trustHeartbeatTimer =
    mode === "full"
      ? setInterval(() => {
          void sendPairingHeartbeat().catch(() => {});
        }, TRUST_HEARTBEAT_INTERVAL_MS)
      : null;
  const browserClientHeartbeatTimer =
    mode === "full"
      ? setInterval(() => {
          const health = connection.health;
          if (health?.ok) void registerBrowserHostClient(health);
        }, BROWSER_CLIENT_HEARTBEAT_INTERVAL_MS)
      : null;

  void (async () => {
    let health: DaemonHealth;
    try {
      await workshops.load();
      if (!recoveryIsCurrent(generation)) return;
      connection.setHealth(null);
      options.onHealthChange(null);
      await ensureMobileDaemonUrl();
      void sendPairingHeartbeat().catch(() => {});
      // P0.1 — day-2+ launch: spawn local engine when health is down (wizard warm
      // only runs while the first-run sheet is visible).
      health = await ensureWorkshopEngineHealthy({
        allowSpawn: mode === "full" && workshops.activeWorkshop?.kind === "local",
      });
      if (!recoveryIsCurrent(generation)) return;
      connection.setHealth(health);
      options.onHealthChange(health);
    } catch (err) {
      if (!recoveryIsCurrent(generation)) return;
      const failed = {
        ok: false,
        message: err instanceof Error ? err.message : String(err),
      };
      connection.setHealth(failed);
      options.onHealthChange(failed);
      scheduleWorkshopConnectionRecovery(options.onHealthChange);
      return;
    }

    void loadWorkshopDefaults(health.ok);
    if (!health.ok) scheduleWorkshopConnectionRecovery(options.onHealthChange);

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
      scheduleWorkshopConnectionRecovery(options.onHealthChange);
    }
  })();

  return () => {
    workshopTeardown = true;
    connectionReconnect.teardown();
    detachForeground();
    if (trustHeartbeatTimer !== null) {
      clearInterval(trustHeartbeatTimer);
    }
    if (browserClientHeartbeatTimer !== null) {
      clearInterval(browserClientHeartbeatTimer);
    }
    Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    if (mode === "full") {
      cancelScheduledStreamRecovery();
      environmentReconnect.teardown();
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
