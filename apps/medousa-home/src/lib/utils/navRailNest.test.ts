import { describe, expect, it } from "vitest";
import {
  LME_MODE_TAB_KIND,
  lmeModeSupportsRailNest,
  NAV_RAIL_NEST_LIMIT,
  nestKeyForLmeMode,
  surfaceSupportsRailNest,
} from "./navRailNest";

describe("navRailNest", () => {
  it("supports hierarchy destinations", () => {
    expect(surfaceSupportsRailNest("chat")).toBe(true);
    expect(surfaceSupportsRailNest("peers")).toBe(true);
    expect(surfaceSupportsRailNest("web")).toBe(true);
    expect(surfaceSupportsRailNest("context")).toBe(false);
  });

  it("supports Work nest (Board / Asks) and leaves other utilities alone", () => {
    expect(surfaceSupportsRailNest("work")).toBe(true);
    expect(surfaceSupportsRailNest("library")).toBe(false);
    expect(surfaceSupportsRailNest("messaging")).toBe(false);
    expect(surfaceSupportsRailNest("calendar")).toBe(false);
    expect(surfaceSupportsRailNest("settings")).toBe(false);
  });

  it("caps nest size at five", () => {
    expect(NAV_RAIL_NEST_LIMIT).toBe(5);
  });

  it("supports nests for open-tab explorer modes", () => {
    expect(lmeModeSupportsRailNest("notes")).toBe(true);
    expect(lmeModeSupportsRailNest("files")).toBe(true);
    expect(lmeModeSupportsRailNest("history")).toBe(false);
    expect(LME_MODE_TAB_KIND.agents).toBe("manuscript");
    expect(nestKeyForLmeMode("scripts")).toBe("lme:scripts");
  });
});
