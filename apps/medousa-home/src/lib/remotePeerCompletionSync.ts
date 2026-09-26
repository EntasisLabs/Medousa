import { invoke } from "@tauri-apps/api/core";
import { isTauriMobilePlatform } from "$lib/platform";
import { chat } from "$lib/stores/chat.svelte";
import { userProfiles } from "$lib/stores/userProfiles.svelte";
import { workshops } from "$lib/stores/workshops.svelte";
import { PERSONAL_WORKSHOP_ID } from "$lib/types/workshopRegistry";

export interface RemotePeerCompletionSyncResult {
  checked: number;
  applied: number;
  pending: number;
  applied_session_ids: string[];
}

export type RemotePeerCompletionSyncDependencies = {
  invokeSync: () => Promise<RemotePeerCompletionSyncResult>;
  eligible: () => boolean;
  onApplied: (result: RemotePeerCompletionSyncResult) => Promise<void> | void;
  intervalMs?: number;
  setIntervalFn?: (callback: () => void, intervalMs: number) => number;
  clearIntervalFn?: (handle: number) => void;
  addWakeListeners?: (wake: () => void) => () => void;
};

export function createRemotePeerCompletionSync(deps: RemotePeerCompletionSyncDependencies) {
  let timer: number | undefined;
  let inFlight: Promise<void> | undefined;
  let stopped = true;
  let runGeneration = 0;
  const setTimer =
    deps.setIntervalFn ??
    ((callback, intervalMs) => setInterval(callback, intervalMs) as unknown as number);
  const clearTimer =
    deps.clearIntervalFn ??
    ((handle) => clearInterval(handle as unknown as ReturnType<typeof setInterval>));
  let removeWakeListeners = () => {};

  const request = (): Promise<void> => {
    if (stopped || !deps.eligible()) return Promise.resolve();
    if (inFlight) return inFlight;
    const requestGeneration = runGeneration;
    const current = (async () => {
      try {
        const result = await deps.invokeSync();
        if (
          !stopped &&
          requestGeneration === runGeneration &&
          deps.eligible() &&
          result.applied_session_ids.length > 0
        ) {
          await deps.onApplied(result);
        }
      } catch {
        // Completion sync is opportunistic; the next foreground/interval retries.
      }
    })();
    inFlight = current;
    void current.finally(() => {
      if (inFlight === current) inFlight = undefined;
    });
    return current;
  };

  return {
    start() {
      if (!stopped) return () => this.stop();
      stopped = false;
      runGeneration += 1;
      removeWakeListeners = deps.addWakeListeners?.(() => void request()) ?? (() => {});
      timer = setTimer(() => void request(), deps.intervalMs ?? 15_000);
      void request();
      return () => this.stop();
    },
    stop() {
      if (stopped) return;
      stopped = true;
      runGeneration += 1;
      if (timer !== undefined) clearTimer(timer);
      timer = undefined;
      removeWakeListeners();
      removeWakeListeners = () => {};
    },
    request,
  };
}

export type PersonalScopeSnapshot = {
  workshopId: string;
  profileId: string | null;
  workshopEpoch: number;
};

function personalScopeSnapshot(): PersonalScopeSnapshot | null {
  if (workshops.activeWorkshopId !== PERSONAL_WORKSHOP_ID) return null;
  if (chat.workshopScopeId !== PERSONAL_WORKSHOP_ID) return null;
  return {
    workshopId: workshops.activeWorkshopId,
    profileId: userProfiles.activeProfileId,
    workshopEpoch: chat.workshopEpoch,
  };
}

function scopeStillMatches(snapshot: PersonalScopeSnapshot): boolean {
  return (
    workshops.activeWorkshopId === snapshot.workshopId &&
    chat.workshopScopeId === snapshot.workshopId &&
    userProfiles.activeProfileId === snapshot.profileId &&
    chat.workshopEpoch === snapshot.workshopEpoch
  );
}

/** Refresh Personal's list and merge the focused transcript without replacing a live stream. */
export async function applyRemotePeerCompletionResult(
  result: RemotePeerCompletionSyncResult,
  deps: {
    personalScope: () => PersonalScopeSnapshot | null;
    scopeStillMatches: (snapshot: PersonalScopeSnapshot) => boolean;
    expectedScope?: PersonalScopeSnapshot | null;
    focusedSessionId: () => string;
    refreshSessions: () => Promise<unknown>;
    reconcileOnResume: () => Promise<unknown>;
  },
): Promise<void> {
  if (result.applied_session_ids.length === 0) return;
  const snapshot = deps.personalScope();
  if (!snapshot) return;
  if ("expectedScope" in deps) {
    if (!deps.expectedScope || !deps.scopeStillMatches(deps.expectedScope)) return;
  }
  const focusedSessionId = deps.focusedSessionId();
  await deps.refreshSessions();
  if (!deps.scopeStillMatches(snapshot)) return;
  if (
    focusedSessionId &&
    deps.focusedSessionId() === focusedSessionId &&
    result.applied_session_ids.includes(focusedSessionId)
  ) {
    await deps.reconcileOnResume();
  }
}

let requestScope: PersonalScopeSnapshot | null = null;
const singleton = createRemotePeerCompletionSync({
  invokeSync: () => {
    requestScope = personalScopeSnapshot();
    return invoke<RemotePeerCompletionSyncResult>("coordination_sync_remote_peer_completions");
  },
  eligible: () =>
    isTauriMobilePlatform() &&
    typeof document !== "undefined" &&
    document.visibilityState === "visible",
  onApplied: (result) =>
    applyRemotePeerCompletionResult(result, {
      personalScope: personalScopeSnapshot,
      scopeStillMatches,
      expectedScope: requestScope,
      focusedSessionId: () => chat.focusedSessionId,
      refreshSessions: () => chat.refreshSessions({ force: true }),
      reconcileOnResume: () => chat.reconcileOnResume({ notice: false }),
    }),
  addWakeListeners: (wake) => {
    const onVisibility = () => {
      if (document.visibilityState === "visible") wake();
    };
    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener("focus", wake);
    window.addEventListener("online", wake);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener("focus", wake);
      window.removeEventListener("online", wake);
    };
  },
});

/** AppShell owns the poller; proposal refreshes can nudge the same serialized sync. */
export function startRemotePeerCompletionSync(): () => void {
  if (!isTauriMobilePlatform()) return () => {};
  return singleton.start();
}

export function requestRemotePeerCompletionSync(): Promise<void> {
  return singleton.request();
}
