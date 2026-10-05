import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  loadDraftForSession,
  persistDraftForSession,
} from "$lib/chat/draftPersistence";
import {
  loadPromotedAskIds,
  loadSessionId,
  savePromotedAskIds,
  SESSION_KEY,
} from "$lib/chat/sessionController";
import { UserProfilesStore } from "$lib/stores/userProfiles.svelte";
import { WorkshopsStore } from "$lib/stores/workshops.svelte";
import { defaultWorkshopRegistry } from "$lib/types/workshopRegistry";
import { setWorkshopRefreshPort } from "$lib/runtime/workshopReconnectPort";
import {
  getSessionAgentRuntime,
  setSessionAgentRuntime,
} from "$lib/utils/sessionAgentRuntime";
import {
  setActiveWorkshopIdPort,
  workshopScopedStorageKey,
} from "$lib/utils/workshopLocality";

describe("workshop client-state isolation", () => {
  let activeWorkshop = "authority-personal";
  let values: Map<string, string>;

  beforeEach(() => {
    values = new Map();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    });
    setActiveWorkshopIdPort(() => activeWorkshop);
  });

  afterEach(() => {
    setWorkshopRefreshPort(null);
    setActiveWorkshopIdPort(null);
    vi.unstubAllGlobals();
  });

  it("does not alias drafts, sessions, promoted asks, or agent config", () => {
    const sessionId = "session-canary";

    persistDraftForSession(sessionId, "personal draft");
    localStorage.setItem(workshopScopedStorageKey(SESSION_KEY), "personal-session");
    savePromotedAskIds(new Set(["ask-personal"]));
    setSessionAgentRuntime(sessionId, "codex");

    activeWorkshop = "authority-remote";
    persistDraftForSession(sessionId, "remote draft");
    localStorage.setItem(workshopScopedStorageKey(SESSION_KEY), "remote-session");
    savePromotedAskIds(new Set(["ask-remote"]));
    setSessionAgentRuntime(sessionId, "hermes");

    expect(loadDraftForSession(sessionId)).toBe("remote draft");
    expect(loadSessionId()).toBe("remote-session");
    expect([...loadPromotedAskIds()]).toEqual(["ask-remote"]);
    expect(getSessionAgentRuntime(sessionId)).toBe("hermes");

    activeWorkshop = "authority-personal";
    expect(loadDraftForSession(sessionId)).toBe("personal draft");
    expect(loadSessionId()).toBe("personal-session");
    expect([...loadPromotedAskIds()]).toEqual(["ask-personal"]);
    expect(getSessionAgentRuntime(sessionId)).toBe("codex");
  });

  it("rejects a profile canary from another workshop", () => {
    const profiles = new UserProfilesStore();
    profiles.workshopScopeId = "authority-personal";
    profiles.profiles = [
      {
        profile_id: "user:personal-canary",
        display_name: "Personal canary",
        created_at: "2026-01-01T00:00:00Z",
        is_default: true,
      },
    ];
    profiles.activeProfileId = "user:remote-canary";

    expect(() => profiles.turnIdentityUserId()).toThrow(
      "selected profile does not belong to the active workshop",
    );

    profiles.activeProfileId = "user:personal-canary";
    expect(profiles.turnIdentityUserId()).toBe("user:personal-canary");

    activeWorkshop = "authority-remote";
    expect(() => profiles.turnIdentityUserId()).toThrow(
      "Profiles are still loading for the active workshop",
    );
  });

  it("records a pairing without selecting the remote workshop", async () => {
    const store = new WorkshopsStore();
    const registry = defaultWorkshopRegistry();
    registry.workshops.push({
      ...registry.workshops[0],
      id: "paired-remote-canary",
      label: "Remote canary",
      kind: "portal",
      url: "https://remote-canary.invalid",
      pairing: {
        pairingId: "pair-canary",
        phoneId: "phone-canary",
        workshopDeviceId: "daemon-canary",
        pairedAt: "2026-01-01T00:00:00Z",
      },
    });
    vi.spyOn(store, "load").mockImplementation(async () => {
      store.registry = registry;
    });
    const select = vi.spyOn(store, "selectWorkshop").mockResolvedValue(undefined);

    await store.onPairComplete({
      pairingId: "pair-canary",
      phoneId: "phone-canary",
      workshopDeviceId: "daemon-canary",
      workshopId: "paired-remote-canary",
      workshopPeerName: "Remote canary",
      daemonUrl: "https://remote-canary.invalid",
    });

    expect(store.activeWorkshopId).toBe("personal");
    expect(store.pendingSwitchAfterPair).toBe("paired-remote-canary");
    expect(select).not.toHaveBeenCalled();
  });

  it("saves the chosen pairing name before offering to switch", async () => {
    const store = new WorkshopsStore();
    const registry = defaultWorkshopRegistry();
    registry.workshops.push({ ...registry.workshops[0], id: "paired-remote", label: "Advertised name", kind: "portal" });
    vi.spyOn(store, "load").mockImplementation(async () => { store.registry = registry; });
    const rename = vi.spyOn(store, "renameWorkshop").mockImplementation(async (id, label) => {
      store.registry.workshops.find((workshop) => workshop.id === id)!.label = label;
    });
    await store.onPairComplete({
      workshopId: "paired-remote", workshopPeerName: "Advertised name",
    } as Parameters<typeof store.onPairComplete>[0], "  Studio Mac  ");
    expect(rename).toHaveBeenCalledWith("paired-remote", "Studio Mac");
    expect(store.pendingSwitchAfterPairLabel).toBe("Studio Mac");
    expect(store.activeWorkshopId).toBe("personal");
  });

  it("keeps a successful pairing when saving its display name fails", async () => {
    const store = new WorkshopsStore();
    vi.spyOn(store, "load").mockResolvedValue(undefined);
    vi.spyOn(store, "renameWorkshop").mockRejectedValue(new Error("Storage unavailable"));
    await store.onPairComplete({ workshopId: "paired-remote" } as Parameters<typeof store.onPairComplete>[0], "Studio");
    expect(store.pendingSwitchAfterPair).toBe("paired-remote");
    expect(store.error).toContain("Workshop paired, but its name could not be saved");
  });

  it("refreshes without switching or reloading workshop state, and prevents overlapping transitions", async () => {
    const store = new WorkshopsStore();
    const load = vi.spyOn(store, "load");
    const select = vi.spyOn(store, "selectWorkshop");
    let finish!: () => void;
    const refresh = vi.fn(async () => {
      await new Promise<void>((resolve) => { finish = resolve; });
      return { ok: true, message: "Connected" };
    });
    setWorkshopRefreshPort(refresh);
    const task = store.refreshConnection();
    expect(store.refreshing).toBe(true);
    store.requestSwitch("paired-remote");
    expect(await store.refreshConnection()).toBe(false);
    expect(select).not.toHaveBeenCalled();
    finish();
    expect(await task).toBe(true);
    expect(store.refreshing).toBe(false);
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(load).not.toHaveBeenCalled();
    expect(store.activeWorkshopId).toBe("personal");
  });

  it("surfaces an unsuccessful refresh and permits retry", async () => {
    const store = new WorkshopsStore();
    const refresh = vi.fn().mockResolvedValueOnce({ ok: false, message: "Workshop offline" }).mockResolvedValue({ ok: true });
    setWorkshopRefreshPort(refresh);
    expect(await store.refreshConnection()).toBe(false);
    expect(store.error).toBe("Workshop offline");
    expect(store.refreshing).toBe(false);
    expect(await store.refreshConnection()).toBe(true);
    expect(store.error).toBeNull();
  });
});
