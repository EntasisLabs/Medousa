import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const f = vi.hoisted(() => ({
  calls: [] as string[],
  health: { ok: true, message: "Connected" },
  probe: vi.fn(),
  loadEnvironment: vi.fn(),
  notify: vi.fn(),
  chat: { sessionId: "session-1", draft: "Keep this draft", messages: [{ content: "Keep this conversation" }] },
  vault: { selectedPath: "notes/draft.md", content: "Unsaved edits", dirty: true },
}));
vi.mock("$lib/daemonConnection", () => ({ ensureMobileDaemonUrl: async () => {} }));
vi.mock("$lib/utils/pairingClient", () => ({
  sendPairingHeartbeat: async () => { f.calls.push("credentials"); },
}));
vi.mock("$lib/utils/ensureWorkshopEngine", () => ({ ensureWorkshopEngineHealthy: f.probe }));
vi.mock("$lib/stores/connection.svelte", () => ({
  connection: {
    setRecovering: (value: boolean) => { f.calls.push(`recovering:${value}`); },
    setHealth: () => {},
  },
}));
vi.mock("$lib/stores/workshops.svelte", () => ({ workshops: { activeWorkshop: { kind: "portal" } } }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: {
  ...f.chat,
  stopOwnedInteractiveStreams: async () => { f.calls.push("stop-interactive"); },
  refreshSessions: async () => {}, reconcileOnResume: async () => {}, hydrateAskThreads: async () => {},
  tryReattachActiveTurn: async () => { f.calls.push("reattach"); return true; },
} }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: {
  ...f.vault, refreshVaultRoots: async () => {}, refreshNotes: async () => {},
} }));
vi.mock("$lib/stores/workspace.svelte", () => ({ workspace: {
  cards: [], revision: 42,
  reconcileCardsFromSnapshot: async () => { f.calls.push("snapshot"); },
  recoverPendingWorkerResults: async () => {},
} }));
vi.mock("$lib/stores/environment.svelte", () => ({
  environment: { load: f.loadEnvironment },
  stopEnvironmentSync: async () => { f.calls.push("stop-environment"); },
  startEnvironmentSync: async () => { f.calls.push("start-environment"); },
}));
vi.mock("$lib/platform", () => ({ isBrowserWorkshop: () => true, isTauriMobilePlatform: () => false }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { refresh: async () => {} } }));
vi.mock("$lib/stores/executionTargets.svelte", () => ({ executionTargets: { refresh: async () => {} } }));
vi.mock("$lib/stores/userProfiles.svelte", () => ({ userProfiles: { syncOnResume: async () => {} } }));
// Unused bootstrap/reset dependencies must not be touched by same-workshop refresh.
vi.mock("$lib/stores/automations.svelte", () => ({ automations: {} }));
vi.mock("$lib/stores/runtime.svelte", () => ({ runtime: {} }));
vi.mock("$lib/stores/settings.svelte", () => ({ settings: {} }));
vi.mock("$lib/stores/workshopDefaults.svelte", () => ({ workshopDefaults: {} }));
vi.mock("$lib/stores/voicePresets.svelte", () => ({ voicePresets: {} }));
vi.mock("$lib/stores/identity.svelte", () => ({ identity: {} }));

// The workspace pipe wrappers below delegate through daemon's exported functions.
vi.mock("$lib/daemon", async () => ({
  invalidateRouteCaches: async () => { f.calls.push("routes"); },
  stopWorkspaceStream: async () => { f.calls.push("stop-workspace"); },
  startWorkspaceStream: async () => { f.calls.push("start-workspace"); },
}));

import { refreshWorkshopConnection } from "./workshopConnection";
import { chat } from "$lib/stores/chat.svelte";
import { vault } from "$lib/stores/vault.svelte";

beforeEach(() => {
  vi.useFakeTimers();
  f.calls.length = 0;
  f.health = { ok: true, message: "Connected" };
  f.probe.mockReset();
  f.loadEnvironment.mockReset();
  f.probe.mockImplementation(async () => { f.calls.push("health"); return f.health; });
  f.loadEnvironment.mockResolvedValue(undefined);
  f.notify.mockClear();
});
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe("refreshWorkshopConnection", () => {
  it("renews routes/credentials before probing, then recovers streams without clearing open work", async () => {
    expect(await refreshWorkshopConnection(f.notify)).toEqual(f.health);
    expect(f.calls.indexOf("routes")).toBeLessThan(f.calls.indexOf("credentials"));
    expect(f.calls.indexOf("credentials")).toBeLessThan(f.calls.indexOf("health"));
    expect(f.probe).toHaveBeenCalledWith({ allowSpawn: false });
    expect(f.calls).toEqual(expect.arrayContaining(["stop-interactive", "snapshot", "start-workspace", "start-environment", "reattach"]));
    expect(chat.sessionId).toBe("session-1");
    expect(chat.draft).toBe("Keep this draft");
    expect(chat.messages).toEqual(f.chat.messages);
    expect(vault.content).toBe("Unsaved edits");
    expect(vault.selectedPath).toBe("notes/draft.md");
    expect(vault.dirty).toBe(true);
    expect(f.calls.at(-1)).toBe("recovering:false");
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
