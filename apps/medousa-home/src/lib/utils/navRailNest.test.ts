import { describe, expect, it } from "vitest";
import { NAV_RAIL_NEST_LIMIT, surfaceSupportsRailNest } from "./navRailNest";

describe("navRailNest", () => {
  it("supports hierarchy destinations", () => {
    expect(surfaceSupportsRailNest("chat")).toBe(true);
    expect(surfaceSupportsRailNest("peers")).toBe(true);
    expect(surfaceSupportsRailNest("library")).toBe(true);
    expect(surfaceSupportsRailNest("web")).toBe(true);
    expect(surfaceSupportsRailNest("context")).toBe(true);
  });

  it("leaves work and utility surfaces alone", () => {
    expect(surfaceSupportsRailNest("work")).toBe(false);
    expect(surfaceSupportsRailNest("messaging")).toBe(false);
    expect(surfaceSupportsRailNest("calendar")).toBe(false);
    expect(surfaceSupportsRailNest("settings")).toBe(false);
  });

  it("caps nest size at five", () => {
    expect(NAV_RAIL_NEST_LIMIT).toBe(5);
  });
});
