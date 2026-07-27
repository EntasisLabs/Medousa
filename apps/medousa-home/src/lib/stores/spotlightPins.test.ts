import { beforeEach, describe, expect, it } from "vitest";
import { spotlightPins } from "./spotlightPins.svelte";

describe("spotlightPins.ensureWorkshopSynced", () => {
  beforeEach(() => {
    spotlightPins.clear();
  });

  it("does not reassign slots when workshop is unchanged", () => {
    spotlightPins.pin({
      kind: "note",
      target: "notes/a.md",
      label: "A",
    });
    const before = spotlightPins.slots;
    spotlightPins.ensureWorkshopSynced();
    spotlightPins.ensureWorkshopSynced();
    expect(spotlightPins.slots).toBe(before);
  });
});
