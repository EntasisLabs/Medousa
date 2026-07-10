import { describe, expect, it } from "vitest";
import { statusPill } from "./statusPill";
import { createNode, registry, validateNode } from "$lib/liquid/core";
import { hasComponent } from "$lib/liquid/render/componentRegistry";

describe("status_pill archetype", () => {
  it("self-registers descriptor + component", () => {
    expect(registry.has("status_pill")).toBe(true);
    expect(hasComponent("status_pill")).toBe(true);
    expect(statusPill.tier).toBe("atom");
    expect(statusPill.acceptsBindings).toContain("feed:id");
  });

  it("validates a node with a label", () => {
    const node = createNode({
      id: "s",
      type: "status_pill",
      props: { label: "Searching…", state: "loading" },
    });
    expect(validateNode(node)).toEqual([]);
  });

  it("flags a missing required label", () => {
    const issues = validateNode(createNode({ id: "s", type: "status_pill" }));
    expect(issues.map((i) => i.code)).toContain("missing_required_prop");
  });
});
