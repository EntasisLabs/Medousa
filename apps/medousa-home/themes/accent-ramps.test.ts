import { describe, expect, it } from "vitest";
import { accentRampProperties, expandAccentRamp } from "./accent-ramps";

describe("expandAccentRamp", () => {
  it("keeps 500 as the base and lightens/darkens around it", () => {
    const ramp = expandAccentRamp("10 132 255");
    expect(ramp["500"]).toBe("10 132 255");
    const [r50] = ramp["50"].split(" ").map(Number);
    const [r900] = ramp["900"].split(" ").map(Number);
    expect(r50).toBeGreaterThan(10);
    expect(r900).toBeLessThan(10);
  });

  it("emits primary-* properties for theme configs", () => {
    const props = accentRampProperties("primary", "72 72 74");
    expect(props["--color-primary-500"]).toBe("72 72 74");
    expect(props["--color-primary-300"]).not.toBe("72 72 74");
    expect(props["--color-primary-700"]).not.toBe("72 72 74");
  });
});
