import { describe, expect, it } from "vitest";
import { componentIdsToPruneOnSave } from "./layoutEdit.svelte";

describe("componentIdsToPruneOnSave", () => {
  it("includes explicitly removed widgets even when filtered from active ids", () => {
    const kept = new Set(["b"]);
    expect(componentIdsToPruneOnSave(["a"], ["a", "b"], kept)).toEqual(["a"]);
  });

  it("drops widgets no longer assigned to any pane", () => {
    const kept = new Set(["a"]);
    expect(componentIdsToPruneOnSave([], ["a", "b"], kept)).toEqual(["b"]);
  });

  it("dedupes overlap between removedDuringEdit and unassigned ids", () => {
    const kept = new Set<string>();
    expect(componentIdsToPruneOnSave(["a"], ["a"], kept)).toEqual(["a"]);
  });
});
