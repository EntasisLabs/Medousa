import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceStreamEvent } from "$lib/types/workspace";
import type { EnvironmentStreamEvent } from "$lib/types/environment";

const f = vi.hoisted(() => ({
  calls: [] as string[],
  health: { ok: true, message: "Connected" },
  probe: vi.fn(), heartbeat: vi.fn(), snapshot: vi.fn(), noteTraffic: vi.fn(),
  loadEnvironment: vi.fn(),
  notify: vi.fn(),
  directHealth: vi.fn(), setHealth: vi.fn(), startWorkspace: vi.fn(), startEnvironment: vi.fn(), recoverWorkers: vi.fn(),
  workspaceError: undefined as ((error: { message: string }) => void) | undefined,
  environmentError: undefined as ((error: { message: string }) => void) | undefined,
  workspaceEvent: undefined as ((event: WorkspaceStreamEvent) => void) | undefined,
  environmentEvent: undefined as ((event: EnvironmentStreamEvent) => void) | undefined,
  chat: { sessionId: "session-1", draft: "Keep this draft", messages: [{ content: "Keep this conversation" }] },
  vault: { selectedPath: "notes/draft.md", content: "Unsaved edits", dirty: true },
}));
vi.mock("$lib/daemonConnection", () => ({ ensureMobileDaemonUrl: async () => {} }));
vi.mock("$lib/utils/pairingClient", () => ({
  sendPairingHeartbeat: f.heartbeat,
}));
vi.mock("$lib/utils/ensureWorkshopEngine", () => ({ ensureWorkshopEngineHealthy: f.probe }));
vi.mock("$lib/stores/connection.svelte", () => ({
  connection: {
    setRecovering: (value: boolean) => { f.calls.push(`recovering:${value}`); },
    setHealth: f.setHealth, noteTraffic: f.noteTraffic, trafficRevision: 0,
  },
}));
vi.mock("$lib/stores/workshops.svelte", () => ({ workshops: {
  activeWorkshop: { kind: "portal" }, load: async () => {},
  restoreLastSession: async () => {}, applyThemeForActiveWorkshop: () => { f.calls.push("theme"); },
} }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: {
  ...f.chat,
  sessionPristine: true, setStreamRole: () => {}, noteResumeFailure: vi.fn(),
  stopOwnedInteractiveStreams: async () => { f.calls.push("stop-interactive"); },
  ensureSessionHydrated: async () => {}, hydrateAskThreads: async () => {},
  refreshSessions: async () => {}, reconcileOnResume: async () => {},
  tryReattachActiveTurn: async () => { f.calls.push("reattach"); return true; },
} }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: {
  ...f.vault, refreshVaultRoots: async () => {}, refreshNotes: async () => {},
} }));
vi.mock("$lib/stores/workspace.svelte", () => ({ workspace: {
  cards: [], revision: 42, applyEvent: vi.fn(), setError: vi.fn(),
  syncTurnWorkerCardsToChat: async () => {},
  reconcileCardsFromSnapshot: f.snapshot,
  recoverPendingWorkerResults: f.recoverWorkers,
} }));
vi.mock("$lib/stores/environment.svelte", () => ({
  environment: { load: f.loadEnvironment, applyEvent: vi.fn(), setError: vi.fn() },
  stopEnvironmentSync: async () => { f.calls.push("stop-environment"); },
  startEnvironmentSync: f.startEnvironment,
}));
vi.mock("$lib/platform", () => ({ isBrowserWorkshop: () => true, isTauriMobilePlatform: () => false }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { refresh: async () => {} } }));
vi.mock("$lib/stores/executionTargets.svelte", () => ({ executionTargets: { refresh: async () => {} } }));
vi.mock("$lib/stores/userProfiles.svelte", () => ({ userProfiles: { syncOnResume: async () => {}, load: async () => {} } }));
// Unused bootstrap/reset dependencies must not be touched by same-workshop refresh.
vi.mock("$lib/stores/automations.svelte", () => ({ automations: { refresh: async () => {} } }));
vi.mock("$lib/stores/runtime.svelte", () => ({ runtime: { loadWorkshopRuntime: async () => {}, refresh: async () => {} } }));
vi.mock("$lib/stores/settings.svelte", () => ({ settings: { applyTheme: () => {}, hydrateWorkRetentionFromDaemon: async () => {} } }));
vi.mock("$lib/stores/workshopDefaults.svelte", () => ({ workshopDefaults: { load: async () => {}, loaded: false } }));
vi.mock("$lib/stores/voicePresets.svelte", () => ({ voicePresets: { load: async () => {} } }));
vi.mock("$lib/stores/identity.svelte", () => ({ identity: {} }));

// The workspace pipe wrappers below delegate through daemon's exported functions.
vi.mock("$lib/daemon", async () => ({
  checkDaemonHealth: f.directHealth,
  onEnvironmentEvent: async (handler: typeof f.environmentEvent) => { f.environmentEvent = handler; return () => {}; },
  onEnvironmentError: async (handler: typeof f.environmentError) => { f.environmentError = handler; return () => {}; },
  onWorkspaceEvent: async (handler: typeof f.workspaceEvent) => { f.workspaceEvent = handler; return () => {}; },
  onWorkspaceError: async (handler: typeof f.workspaceError) => { f.workspaceError = handler; return () => {}; },
  onInteractiveEvent: async () => () => {}, onInteractiveError: async () => () => {},
  invalidateRouteCaches: async () => { f.calls.push("routes"); },
  stopWorkspaceStream: async () => { f.calls.push("stop-workspace"); },
  startWorkspaceStream: f.startWorkspace,
}));

import { connectWorkshop, refreshWorkshopConnection, resumeWorkshop } from "./workshopConnection";
import { chat } from "$lib/stores/chat.svelte";
import { vault } from "$lib/stores/vault.svelte";

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("document", { visibilityState: "visible", addEventListener: vi.fn(), removeEventListener: vi.fn() });
  f.calls.length = 0;
  f.health = { ok: true, message: "Connected" };
  f.heartbeat.mockReset(); f.snapshot.mockReset(); f.noteTraffic.mockClear();
  f.heartbeat.mockImplementation(async () => { f.calls.push("credentials"); });
  f.snapshot.mockImplementation(async () => { f.calls.push("snapshot"); });
  f.probe.mockReset();
  f.loadEnvironment.mockReset();
  f.probe.mockImplementation(async () => { f.calls.push("health"); return f.health; });
  f.loadEnvironment.mockResolvedValue(undefined);
  f.notify.mockClear(); f.directHealth.mockReset(); f.setHealth.mockClear();
  f.startWorkspace.mockReset(); f.startEnvironment.mockReset(); f.recoverWorkers.mockReset();
  f.startWorkspace.mockImplementation(async () => { f.calls.push("start-workspace"); });
  f.startEnvironment.mockImplementation(async () => { f.calls.push("start-environment"); });
  f.recoverWorkers.mockResolvedValue(undefined);
  f.workspaceError = undefined; f.environmentError = undefined;
  f.workspaceEvent = undefined; f.environmentEvent = undefined;
});

afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("refreshWorkshopConnection", () => {
  it("renews routes/credentials before probing, then recovers streams without clearing open work", async () => {
    expect(await refreshWorkshopConnection(f.notify)).toEqual(f.health);
    expect(f.calls.indexOf("routes")).toBeLessThan(f.calls.indexOf("credentials"));
    expect(f.calls.indexOf("credentials")).toBeLessThan(f.calls.indexOf("health"));
    expect(f.probe).toHaveBeenCalledWith({ allowSpawn: false });
    expect(f.calls.indexOf("start-workspace")).toBeLessThan(f.calls.indexOf("snapshot"));
    expect(f.calls).toEqual(expect.arrayContaining(["stop-interactive", "snapshot", "start-workspace", "start-environment", "reattach"]));
    expect(chat.sessionId).toBe("session-1");
    expect(chat.draft).toBe("Keep this draft");
    expect(chat.messages).toEqual(f.chat.messages);
    expect(vault.content).toBe("Unsaved edits");
    expect(vault.selectedPath).toBe("notes/draft.md");
    expect(vault.dirty).toBe(true);
    expect(f.calls.at(-1)).toBe("recovering:false");
  });

  it("opens streams while heartbeat and projection requests are still pending", async () => {
    f.heartbeat.mockReturnValueOnce(new Promise(() => {}));
    let resolve!: () => void;
    f.snapshot.mockImplementationOnce(() => new Promise<void>((done) => { resolve = done; }));
    const refresh = refreshWorkshopConnection(f.notify);
    await vi.waitFor(() => expect(f.startWorkspace).toHaveBeenCalled());
    expect(f.startEnvironment).toHaveBeenCalled();
    await vi.waitFor(() => expect(resolve).toBeDefined());
    resolve();
    await refresh;
  });

  it("keeps existing pipes and open work when the workshop is unreachable", async () => {
    f.health = { ok: false, message: "Offline" };
    expect(await refreshWorkshopConnection(f.notify)).toEqual(f.health);
    expect(f.calls).not.toContain("stop-interactive");
    expect(f.calls).not.toContain("stop-workspace");
    expect(vault.content).toBe("Unsaved edits");
    expect(f.notify).toHaveBeenCalledWith(f.health);
  });

  it("restarts pipes and releases busy state even when a projection refresh fails", async () => {
    f.loadEnvironment.mockRejectedValueOnce(new Error("Snapshot failed"));
    await expect(refreshWorkshopConnection(f.notify)).rejects.toThrow("Snapshot failed");
    expect(f.calls).toEqual(expect.arrayContaining(["start-workspace", "start-environment"]));
    expect(f.calls.at(-1)).toBe("recovering:false");
    expect(await refreshWorkshopConnection(f.notify)).toEqual(f.health);
  });

  it("shares a concurrent refresh instead of stopping/restarting pipes twice", async () => {
    let resolve!: (health: typeof f.health) => void;
    f.probe.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const first = refreshWorkshopConnection(f.notify);
    const second = refreshWorkshopConnection(f.notify);
    await vi.waitFor(() => expect(f.probe).toHaveBeenCalled());
    resolve(f.health);
    await Promise.all([first, second]);
    expect(f.calls.filter((call) => call === "routes")).toHaveLength(1);
    expect(f.calls.filter((call) => call === "start-workspace")).toHaveLength(1);
    expect(f.notify).toHaveBeenCalledTimes(2);
  });
});

describe("foreground recovery", () => {
  it("remote streams reopen before a stalled health probe finishes", async () => {
    const detach = connectWorkshop({ onHealthChange: f.notify });
    await vi.waitFor(() => expect(f.calls).toContain("theme"));
    f.calls.length = 0;
    f.probe.mockClear();
    let resolve!: (health: typeof f.health) => void;
    f.probe.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const resume = resumeWorkshop(f.notify);
    await vi.waitFor(() => expect(f.probe).toHaveBeenCalled());
    expect(f.calls).toContain("start-workspace");
    expect(f.calls).toContain("start-environment");
    expect(f.calls).not.toContain("snapshot");
    resolve(f.health);
    await resume;
    await detach();
  });
});

describe("initial workshop connection recovery", () => {
  it("recovers a failed first health probe without an SSE error or another foreground event", async () => {
    f.probe.mockResolvedValueOnce({ ok: false, message: "Iroh handshake timed out" });
    const detach = connectWorkshop({ onHealthChange: f.notify });
    try {
      await vi.waitFor(() => expect(f.notify).toHaveBeenCalledWith(expect.objectContaining({ ok: false })));
      expect(f.calls).not.toContain("start-workspace");
      await vi.advanceTimersByTimeAsync(1000);
      await vi.waitFor(() => expect(f.notify).toHaveBeenCalledWith(f.health));
      expect(f.calls).toEqual(expect.arrayContaining(["snapshot", "start-workspace", "start-environment", "reattach"]));
      expect(chat.draft).toBe("Keep this draft");
      expect(vault.content).toBe("Unsaved edits");
    } finally { detach(); }
  });

  it("cancels a scheduled initial recovery when its shell is disposed", async () => {
    f.probe.mockResolvedValueOnce({ ok: false, message: "Offline" });
    const detach = connectWorkshop({ onHealthChange: f.notify });
    await vi.waitFor(() => expect(f.probe).toHaveBeenCalledTimes(1));
    detach();
    await vi.advanceTimersByTimeAsync(10000);
    expect(f.probe).toHaveBeenCalledTimes(1);
  });

  it("ignores a late startup health result after its shell is disposed", async () => {
    let resolve!: (health: typeof f.health) => void;
    f.probe.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const detach = connectWorkshop({ onHealthChange: f.notify });
    await vi.waitFor(() => expect(f.probe).toHaveBeenCalledTimes(1));
    detach();
    resolve(f.health);
    await vi.advanceTimersByTimeAsync(1000);
    expect(f.notify).not.toHaveBeenCalledWith(f.health);
    expect(f.calls).not.toContain("start-workspace");
  });
});


describe("quiet background stream recovery", () => {
  async function connected() {
    const detach = connectWorkshop({ onHealthChange: f.notify });
    await vi.waitFor(() => expect(f.calls).toContain("theme"));
    f.calls.length = 0;
    f.loadEnvironment.mockClear(); f.setHealth.mockClear(); f.notify.mockClear(); f.recoverWorkers.mockClear();
    return detach;
  }

  it("reconnects a failed workspace stream from its cursor without health checks or UI refreshes", async () => {
    const detach = await connected();
    try {
      f.workspaceError!({ message: "Connection closed" });
      await vi.advanceTimersByTimeAsync(500);
      expect(f.startWorkspace).toHaveBeenLastCalledWith(42);
      expect(f.probe).toHaveBeenCalledTimes(1);
      expect(f.directHealth).not.toHaveBeenCalled();
      expect(f.setHealth).not.toHaveBeenCalled();
      expect(f.notify).not.toHaveBeenCalled();
      expect(f.calls).not.toContain("snapshot");
      expect(f.calls).not.toContain("reattach");
      expect(f.recoverWorkers).not.toHaveBeenCalled();
      f.workspaceEvent!({ workspace_revision: 42, stream_event_type: "snapshot", emitted_at_utc: "" });
      expect(f.noteTraffic).toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(0);
      expect(f.recoverWorkers).toHaveBeenCalledOnce();
      expect(f.calls).toContain("reattach");
      expect(chat.draft).toBe("Keep this draft");
      expect(chat.messages).toEqual(f.chat.messages);
    } finally { detach(); }
  });

  it("recovers environment and workspace independently without reloading the loaded environment", async () => {
    const detach = await connected();
    try {
      f.environmentError!({ message: "Connection closed" });
      f.workspaceError!({ message: "Connection closed" });
      await vi.advanceTimersByTimeAsync(500);
      expect(f.calls.filter(call => call === "start-workspace")).toHaveLength(1);
      expect(f.calls.filter(call => call === "start-environment")).toHaveLength(1);
      expect(f.loadEnvironment).not.toHaveBeenCalled();
      expect(f.directHealth).not.toHaveBeenCalled();
      expect(f.setHealth).not.toHaveBeenCalled();
      expect(vault.content).toBe("Unsaved edits");
    } finally { detach(); }
  });

  it.each(["workspace", "environment"] as const)("backs off %s failures until real stream data confirms recovery", async (stream) => {
    const detach = await connected();
    const fail = stream === "workspace" ? f.workspaceError! : f.environmentError!;
    const start = stream === "workspace" ? f.startWorkspace : f.startEnvironment;
    start.mockClear();
    try {
      fail({ message: "Handshake failed" });
      await vi.advanceTimersByTimeAsync(500);
      expect(start).toHaveBeenCalledTimes(1);
      fail({ message: "Handshake failed after native start returned" });
      await vi.advanceTimersByTimeAsync(500);
      expect(start).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1338);
      expect(start).toHaveBeenCalledTimes(2);
      if (stream === "workspace") {
        f.workspaceEvent!({ workspace_revision: 42, stream_event_type: "heartbeat", emitted_at_utc: "" });
      } else {
        f.environmentEvent!({ revision: 42, eventType: "heartbeat", emittedAtUtc: "" });
      }
      fail({ message: "New interruption" });
      await vi.advanceTimersByTimeAsync(500);
      expect(start).toHaveBeenCalledTimes(3);
      expect(f.probe).toHaveBeenCalledTimes(1);
      expect(f.directHealth).not.toHaveBeenCalled();
    } finally { detach(); }
  });

  it("does no background reconnection while the app is hidden, and cancels retries on disposal", async () => {
    const detach = await connected();
    f.workspaceError!({ message: "Disconnected" });
    f.environmentError!({ message: "Disconnected" });
    Object.assign(document, { visibilityState: "hidden" });
    await vi.advanceTimersByTimeAsync(10000);
    expect(f.calls).not.toContain("start-workspace");
    expect(f.calls).not.toContain("start-environment");
    Object.assign(document, { visibilityState: "visible" });
    f.workspaceError!({ message: "Disconnected" });
    detach(); await vi.advanceTimersByTimeAsync(10000);
    expect(f.calls).not.toContain("start-workspace");
    expect(f.directHealth).not.toHaveBeenCalled();
  });
});
