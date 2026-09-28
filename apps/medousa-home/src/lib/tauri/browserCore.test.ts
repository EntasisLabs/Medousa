import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it } from "vitest";

describe("browser invoke", () => {
  it("resolves native commands when Tauri internals are absent", async () => {
    await expect(invoke("siri_sync_workshop_snapshot")).resolves.toBeNull();
    await expect(invoke("daemon_url")).resolves.toBe("");
    await expect(invoke("vault_list_notes")).resolves.toEqual({ notes: [] });
  });
});
