import { describe, expect, it } from "vitest";
import { actionRow } from "./actionRow";
import { chip } from "$lib/liquid/archetypes/atoms/chip/chip";
import { createNode, registry, validateNode } from "$lib/liquid/core";
import { hasComponent } from "$lib/liquid/render/componentRegistry";

describe("action_row + chip archetypes", () => {
  it("action_row self-registers and emits submit", () => {
    expect(registry.has("action_row")).toBe(true);
    expect(hasComponent("action_row")).toBe(true);
    expect(actionRow.emits).toContain("submit");
  });

  it("chip self-registers and emits select", () => {
    expect(registry.has("chip")).toBe(true);
    expect(hasComponent("chip")).toBe(true);
    expect(chip.emits).toContain("select");
  });

  it("validates a well-formed action_row", () => {
    const node = createNode({
      id: "a1",
      type: "action_row",
      props: { label: "Compare them", intent: "compare" },
    });
    expect(validateNode(node)).toEqual([]);
  });

  it("flags an action_row missing its label", () => {
    expect(validateNode(createNode({ id: "a1", type: "action_row" })).map((i) => i.code)).toContain(
      "missing_required_prop",
    );
  });
});
