import { describe, expect, it } from "vitest";
import { stack } from "./stack";
import { createNode, registry, validateNode } from "$lib/liquid/core";
import { hasComponent } from "$lib/liquid/render/componentRegistry";

describe("stack archetype", () => {
  it("self-registers descriptor + component", () => {
    expect(registry.has("stack")).toBe(true);
    expect(hasComponent("stack")).toBe(true);
    expect(stack.tier).toBe("layout");
    expect(stack.slots).toContain("children");
  });

  it("validates a stack with a children slot", () => {
    const node = createNode({
      id: "st",
      type: "stack",
      props: { direction: "v" },
      slots: { children: [createNode({ id: "p", type: "prose", props: { markdown: "hi" } })] },
    });
    expect(validateNode(node)).toEqual([]);
  });

  it("flags an unknown slot", () => {
    const node = createNode({ id: "st", type: "stack", slots: { nope: [] } });
    expect(validateNode(node).map((i) => i.code)).toContain("unknown_slot");
  });
});
