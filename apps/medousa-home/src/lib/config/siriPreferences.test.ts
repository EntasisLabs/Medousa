// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("$lib/window", () => ({ isTauri: () => false }));

import {
  DEFAULT_SIRI_PREFERENCES,
  readSiriPreferences,
  writeSiriPreferences,
} from "$lib/config/siriPreferences";

describe("Siri preferences", () => {
  beforeEach(() => localStorage.clear());

  it("uses safe defaults", () => {
    expect(readSiriPreferences()).toEqual(DEFAULT_SIRI_PREFERENCES);
  });

  it("clamps spoken length and rejects unknown speech modes", () => {
    localStorage.setItem(
      "medousa-siri-preferences-v1",
      JSON.stringify({ speechMode: "loud", maxSpokenCharacters: 12 }),
    );
    expect(readSiriPreferences()).toMatchObject({
      speechMode: "auto",
      maxSpokenCharacters: 80,
    });
  });

  it("persists a pinned chat", async () => {
    await writeSiriPreferences({ defaultWorkshopId: "personal", defaultSessionId: "session-1" });
    expect(readSiriPreferences()).toMatchObject({
      defaultWorkshopId: "personal",
      defaultSessionId: "session-1",
    });
    await writeSiriPreferences({ defaultWorkshopId: null, defaultSessionId: null });
    expect(readSiriPreferences().defaultSessionId).toBeNull();
  });
});
