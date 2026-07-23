import { describe, expect, it } from "vitest";
import { resolveSummonToolbarSurface } from "./resolveSummonToolbarSurface";

describe("resolveSummonToolbarSurface", () => {
  it("prefers non-LME list surfaces over leftover explorer mode", () => {
    expect(resolveSummonToolbarSurface("chat", "notes")).toBe("chat");
    expect(resolveSummonToolbarSurface("peers", "scripts")).toBe("peers");
    expect(resolveSummonToolbarSurface("messaging", "history")).toBe("messaging");
    expect(resolveSummonToolbarSurface("context", "notes")).toBe("context");
    expect(resolveSummonToolbarSurface("settings", "agents")).toBe("settings");
    expect(resolveSummonToolbarSurface("web", "notes")).toBe("web");
    expect(resolveSummonToolbarSurface("calendar", "scripts")).toBe("calendar");
    expect(resolveSummonToolbarSurface("work", "notes")).toBe("work");
    expect(resolveSummonToolbarSurface("profiles", "notes")).toBe("profiles");
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
    expect(resolveSummonToolbarSurface("runtime", "notes")).toBeNull();
  });
});
