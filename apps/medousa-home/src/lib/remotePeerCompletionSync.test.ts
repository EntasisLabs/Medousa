import { describe, expect, it, vi } from "vitest";
import {
  applyRemotePeerCompletionResult,
  createRemotePeerCompletionSync,
  type PersonalScopeSnapshot,
  type RemotePeerCompletionSyncResult,
} from "./remotePeerCompletionSync";

const appliedResult: RemotePeerCompletionSyncResult = {
  checked: 1,
  applied: 1,
  pending: 0,
  applied_session_ids: ["source-session"],
};

describe("remote peer completion sync", () => {
  it("starts on visible mobile, skips hidden requests, and resumes on reconnect wake", async () => {
    let eligible = true;
    let wake = () => {};
    const invokeSync = vi.fn(async () => appliedResult);
    const onApplied = vi.fn();
    const service = createRemotePeerCompletionSync({
      invokeSync,
      eligible: () => eligible,
      onApplied,
      addWakeListeners: (handler) => {
        wake = handler;
        return () => { wake = () => {}; };
      },
    });

    const stop = service.start();
    await service.request();
    expect(invokeSync).toHaveBeenCalledTimes(1);
    expect(onApplied).toHaveBeenCalledWith(appliedResult);

    eligible = false;
    await service.request();
    wake();
    expect(invokeSync).toHaveBeenCalledTimes(1);

    eligible = true;
    wake();
    await service.request();
    expect(invokeSync).toHaveBeenCalledTimes(2);
    stop();
  });

  it("coalesces overlapping polls and removes timers and wake listeners on cleanup", async () => {
    let resolveSync!: (value: RemotePeerCompletionSyncResult) => void;
    let intervalCallback: (() => void) | undefined;
    let removed = false;
    const onApplied = vi.fn();
    const invokeSync = vi.fn(
      () => new Promise<RemotePeerCompletionSyncResult>((resolve) => { resolveSync = resolve; }),
    );
    const clearIntervalFn = vi.fn((_handle: number) => {});
    const service = createRemotePeerCompletionSync({
      invokeSync,
      eligible: () => true,
      onApplied,
      setIntervalFn: (callback) => {
        intervalCallback = callback;
        return 5;
      },
      clearIntervalFn,
      addWakeListeners: () => () => { removed = true; },
    });

    const stop = service.start();
    intervalCallback?.();
    const pending = service.request();
    expect(service.request()).toBe(pending);
    expect(invokeSync).toHaveBeenCalledTimes(1);
    stop();
    expect(clearIntervalFn).toHaveBeenCalledWith(5);
    expect(removed).toBe(true);

    resolveSync(appliedResult);
    await pending;
    expect(onApplied).not.toHaveBeenCalled();
    await expect(service.request()).resolves.toBeUndefined();
  });

  it("refreshes only a still-current Personal scope and reconciles the included focused session", async () => {
    let snapshot: PersonalScopeSnapshot | null = { workshopId: "personal", profileId: "profile-a", workshopEpoch: 0 };
    let stillMatches = true;
    const refreshSessions = vi.fn(async () => {});
    const reconcileOnResume = vi.fn(async () => {});
    await applyRemotePeerCompletionResult(appliedResult, {
      personalScope: () => snapshot,
      scopeStillMatches: () => stillMatches,
      focusedSessionId: () => "source-session",
      refreshSessions,
      reconcileOnResume,
    });
    expect(refreshSessions).toHaveBeenCalledOnce();
    expect(reconcileOnResume).toHaveBeenCalledOnce();

    snapshot = null;
    refreshSessions.mockClear();
    reconcileOnResume.mockClear();
    await applyRemotePeerCompletionResult(appliedResult, {
      personalScope: () => snapshot,
      scopeStillMatches: () => stillMatches,
      focusedSessionId: () => "source-session",
      refreshSessions,
      reconcileOnResume,
    });
    expect(refreshSessions).not.toHaveBeenCalled();

    snapshot = { workshopId: "personal", profileId: "profile-a", workshopEpoch: 0 };
    stillMatches = false;
    await applyRemotePeerCompletionResult(appliedResult, {
      personalScope: () => snapshot,
      scopeStillMatches: () => stillMatches,
      focusedSessionId: () => "source-session",
      refreshSessions,
      reconcileOnResume,
    });
    expect(refreshSessions).toHaveBeenCalledOnce();
    expect(reconcileOnResume).not.toHaveBeenCalled();
  });

  it("never reconciles a different focused session", async () => {
    const reconcileOnResume = vi.fn(async () => {});
    await applyRemotePeerCompletionResult(appliedResult, {
      personalScope: () => ({ workshopId: "personal", profileId: null, workshopEpoch: 0 }),
      scopeStillMatches: () => true,
      focusedSessionId: () => "another-session",
      refreshSessions: async () => {},
      reconcileOnResume,
    });
    expect(reconcileOnResume).not.toHaveBeenCalled();
  });

  it("skips UI updates when profile scope changes while the command is in flight", async () => {
    const refreshSessions = vi.fn(async () => {});
    await applyRemotePeerCompletionResult(appliedResult, {
      personalScope: () => ({ workshopId: "personal", profileId: "profile-b", workshopEpoch: 1 }),
      scopeStillMatches: (snapshot) => snapshot.profileId === "profile-b" && snapshot.workshopEpoch === 1,
      expectedScope: { workshopId: "personal", profileId: "profile-a", workshopEpoch: 0 },
      focusedSessionId: () => "source-session",
      refreshSessions,
      reconcileOnResume: async () => {},
    });
    expect(refreshSessions).not.toHaveBeenCalled();
  });

  it("does not reconcile a new focused session selected while the list refreshes", async () => {
    let focusedSessionId = "source-session";
    const reconcileOnResume = vi.fn(async () => {});
    await applyRemotePeerCompletionResult(appliedResult, {
      personalScope: () => ({ workshopId: "personal", profileId: null, workshopEpoch: 0 }),
      scopeStillMatches: () => true,
      focusedSessionId: () => focusedSessionId,
      refreshSessions: async () => { focusedSessionId = "another-session"; },
      reconcileOnResume,
    });
    expect(reconcileOnResume).not.toHaveBeenCalled();
  });
});
