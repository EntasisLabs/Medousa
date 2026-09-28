/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import {
  browserPortalActive,
  expandOperationPath,
  loadBrowserWorkshopRegistry,
  saveBrowserWorkshopRegistry,
  streamPathFromUrl,
} from "./browserPortal";
import { defaultWorkshopRegistry, PERSONAL_WORKSHOP_ID } from "$lib/types/workshopRegistry";

describe("browser portal routing", () => {
  it("expands path parameters and query", () => {
    expect(
      expandOperationPath(
        "/v1/sessions/{session_id}/history",
        { session_id: "abc/def" },
        { limit: "20", cursor: "" },
      ),
    ).toBe("/v1/sessions/abc%2Fdef/history?limit=20");
  });

  it("keeps splat slashes", () => {
    expect(
      expandOperationPath("/v1/vault/files/{*file_path}", { file_path: "notes/today.md" }),
    ).toBe("/v1/vault/files/notes/today.md");
  });

  it("reads a stream path from an absolute ticket url", () => {
    expect(
      streamPathFromUrl("http://127.0.0.1:7419/v1/interactive/turn/turn-1/stream?since=2"),
    ).toBe("/v1/interactive/turn/turn-1/stream?since=2");
  });

  it("treats a saved portal as the active workshop", () => {
    const registry = defaultWorkshopRegistry();
    registry.activeWorkshopId = "paired-abcd";
    registry.workshops.push({
      id: "paired-abcd",
      label: "Desk",
      kind: "portal",
      url: "http://127.0.0.1:7419",
      createdAt: "2026-09-28T00:00:00.000Z",
      updatedAt: "2026-09-28T00:00:00.000Z",
    });
    saveBrowserWorkshopRegistry(registry);
    expect(loadBrowserWorkshopRegistry()?.activeWorkshopId).toBe("paired-abcd");
    expect(browserPortalActive()).toBe(true);

    registry.activeWorkshopId = PERSONAL_WORKSHOP_ID;
    saveBrowserWorkshopRegistry(registry);
    expect(browserPortalActive()).toBe(false);
  });
});
