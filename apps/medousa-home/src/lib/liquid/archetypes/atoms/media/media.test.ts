import { describe, expect, it } from "vitest";
import { media } from "./media";
import { createNode, registry, validateNode } from "$lib/liquid/core";
import { hasComponent } from "$lib/liquid/render/componentRegistry";

describe("media archetype", () => {
  it("self-registers descriptor + component", () => {
    expect(registry.has("media")).toBe(true);
    expect(hasComponent("media")).toBe(true);
    expect(media.acceptsBindings).toContain("artifact:id");
  });

  it("validates a node with a src", () => {
    const node = createNode({ id: "m", type: "media", props: { src: "x.png", alt: "x" } });
    expect(validateNode(node)).toEqual([]);
  });

  it("flags a missing required src", () => {
    const issues = validateNode(createNode({ id: "m", type: "media" }));
    expect(issues.map((i) => i.code)).toContain("missing_required_prop");
  });
});
