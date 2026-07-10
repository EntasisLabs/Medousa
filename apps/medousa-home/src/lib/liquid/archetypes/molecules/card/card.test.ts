import { describe, expect, it } from "vitest";
import { card } from "./card";
import { createNode, registry, validateNode } from "$lib/liquid/core";
import { hasComponent } from "$lib/liquid/render/componentRegistry";

describe("card archetype", () => {
  it("self-registers descriptor + component", () => {
    expect(registry.has("card")).toBe(true);
    expect(hasComponent("card")).toBe(true);
    expect(card.slots).toContain("detail");
    expect(card.emits).toContain("expand");
  });

  it("validates a card with a detail slot", () => {
    const node = createNode({
      id: "c1",
      type: "card",
      props: { title: "Studio 14", badges: ["16GB"] },
      slots: { detail: [createNode({ id: "c1:d", type: "prose", props: { markdown: "specs" } })] },
    });
    expect(validateNode(node)).toEqual([]);
  });

  it("flags a missing required title", () => {
    expect(validateNode(createNode({ id: "c1", type: "card" })).map((i) => i.code)).toContain(
      "missing_required_prop",
    );
  });

  it("flags an unknown slot", () => {
    const node = createNode({ id: "c1", type: "card", props: { title: "x" }, slots: { body: [] } });
    expect(validateNode(node).map((i) => i.code)).toContain("unknown_slot");
  });
});
