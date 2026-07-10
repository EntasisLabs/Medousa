import { describe, expect, it } from "vitest";
import { prose } from "./prose";
import { createNode, registry, validateNode } from "$lib/liquid/core";
import { hasComponent } from "$lib/liquid/render/componentRegistry";

describe("prose archetype", () => {
  it("self-registers descriptor + component", () => {
    expect(registry.has("prose")).toBe(true);
    expect(hasComponent("prose")).toBe(true);
    expect(prose.tier).toBe("atom");
  });

  it("validates a well-formed node", () => {
    expect(validateNode(createNode({ id: "p", type: "prose", props: { markdown: "hi" } }))).toEqual([]);
  });

  it("flags a missing required markdown prop", () => {
    const issues = validateNode(createNode({ id: "p", type: "prose" }));
    expect(issues.map((i) => i.code)).toContain("missing_required_prop");
  });
});
