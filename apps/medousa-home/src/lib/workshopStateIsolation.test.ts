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
    setSessionAgentRuntime(sessionId, "cursor");

    expect(loadDraftForSession(sessionId)).toBe("remote draft");
    expect(loadSessionId()).toBe("remote-session");
    expect([...loadPromotedAskIds()]).toEqual(["ask-remote"]);
    expect(getSessionAgentRuntime(sessionId)).toBe("cursor");

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
});
