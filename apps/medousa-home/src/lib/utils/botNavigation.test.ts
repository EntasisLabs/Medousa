import { describe, expect, it } from "vitest";
import { defaultEnvironmentSpec } from "./environmentDefault";
import { cloneEnvironmentSpec } from "./environmentCanvasOps";
import { botNavigationAccent, setBotNavigationPinned } from "./botNavigation";

describe("shared Bot navigation", () => {
  it("pins once, preserves accents, and unpins without deleting customization", () => {
    const spec = defaultEnvironmentSpec();
    spec.nav = { botPins: [], botAccents: { one: "#12aBC0" } };
    const before = cloneEnvironmentSpec(spec);
    setBotNavigationPinned(spec, "one", true);
    setBotNavigationPinned(spec, "one", true);
    expect(spec.nav.botPins).toEqual(["one"]);
    expect(botNavigationAccent(spec, "one")).toBe("#12aBC0");
    expect(cloneEnvironmentSpec(spec).nav).toEqual(spec.nav);
    setBotNavigationPinned(spec, "one", false);
    expect(spec).toEqual(before);
  });
  it("validates IDs and never exposes arbitrary CSS values", () => {
    const spec = defaultEnvironmentSpec();
    expect(() => setBotNavigationPinned(spec, " bad ", true)).toThrow();
    spec.nav = { botPins: [], botAccents: { one: "var(--arbitrary)" } };
    expect(botNavigationAccent(spec, "one")).toBeUndefined();
  });
});
