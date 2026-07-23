import { describe, expect, it } from "vitest";
import { resolveSummonToolbarSurface } from "./resolveSummonToolbarSurface";

describe("resolveSummonToolbarSurface", () => {
  it("prefers non-LME list surfaces over leftover explorer mode", () => {
    expect(resolveSummonToolbarSurface("chat", "notes")).toBe("chat");
    expect(resolveSummonToolbarSurface("peers", "scripts")).toBe("peers");
    expect(resolveSummonToolbarSurface("messaging", "history")).toBe("messaging");
    expect(resolveSummonToolbarSurface("context", "notes")).toBe("context");
    expect(resolveSummonToolbarSurface("settings", "agents")).toBe("settings");
  });

  it("maps library/workshop desktop to explorer family", () => {
    expect(resolveSummonToolbarSurface("library", "notes")).toBe("library");
    expect(resolveSummonToolbarSurface("library", "files")).toBe("library");
    expect(resolveSummonToolbarSurface("library", "scripts")).toBe("automations");
    expect(resolveSummonToolbarSurface("library", "schedules")).toBe("automations");
    expect(resolveSummonToolbarSurface("workshop", "flows")).toBe("automations");
    expect(resolveSummonToolbarSurface("automations", "history")).toBe("automations");
  });

  it("returns null for surfaces without list chrome", () => {
    expect(resolveSummonToolbarSurface("web", "notes")).toBeNull();
    expect(resolveSummonToolbarSurface("calendar", "scripts")).toBeNull();
    expect(resolveSummonToolbarSurface("work", "notes")).toBeNull();
  });
});
