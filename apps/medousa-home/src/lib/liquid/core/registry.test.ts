import { describe, expect, it } from "vitest";
import {
  ArchetypeRegistry,
  defineArchetype,
  validateNode,
  validateTree,
  type ArchetypeDescriptor,
} from "./registry";
import { createNode } from "./scene";

function proseDesc(): ArchetypeDescriptor {
  return {
    id: "prose",
    tier: "atom",
    props: { markdown: { type: "string", required: true } },
    acceptsBindings: ["inline", "vault:path"],
    writeCapable: false,
    slots: [],
    emits: ["navigate"],
    virtualization: "none",
    defaultOwner: "agent",
  };
}

function sectionDesc(): ArchetypeDescriptor {
  return {
    id: "section",
    tier: "molecule",
    acceptsBindings: [],
    writeCapable: false,
    slots: ["body"],
    emits: [],
    virtualization: "none",
    defaultOwner: "agent",
  };
}

describe("ArchetypeRegistry", () => {
  it("registers, gets, has, and lists", () => {
    const reg = new ArchetypeRegistry();
    defineArchetype(proseDesc(), reg);
    expect(reg.has("prose")).toBe(true);
    expect(reg.get("prose")?.tier).toBe("atom");
    expect(reg.all()).toHaveLength(1);
    reg.clear();
    expect(reg.has("prose")).toBe(false);
  });
});

describe("validateNode", () => {
  const reg = new ArchetypeRegistry();
  defineArchetype(proseDesc(), reg);
  defineArchetype(sectionDesc(), reg);

  it("passes a well-formed node", () => {
    const node = createNode({ id: "p", type: "prose", props: { markdown: "hi" } });
    expect(validateNode(node, reg)).toEqual([]);
  });

  it("flags an unknown archetype", () => {
    const issues = validateNode(createNode({ id: "x", type: "mystery" }), reg);
    expect(issues[0]?.code).toBe("unknown_type");
  });

  it("flags a binding source the archetype does not accept", () => {
    const node = createNode({
      id: "p",
      type: "prose",
      props: { markdown: "hi" },
      binding: { source: "work:board", mode: "read" },
    });
    expect(validateNode(node, reg).map((i) => i.code)).toContain("binding_not_accepted");
  });

  it("flags an unknown slot", () => {
    const node = createNode({
      id: "s",
      type: "section",
      slots: { nope: [] },
    });
    expect(validateNode(node, reg).map((i) => i.code)).toContain("unknown_slot");
  });

  it("flags a missing required prop", () => {
    const node = createNode({ id: "p", type: "prose" });
    expect(validateNode(node, reg).map((i) => i.code)).toContain("missing_required_prop");
  });
});

describe("validateTree", () => {
  const reg = new ArchetypeRegistry();
  defineArchetype(proseDesc(), reg);
  defineArchetype(sectionDesc(), reg);

  it("collects issues across the tree", () => {
    const root = createNode({
      id: "sec",
      type: "section",
      slots: {
        body: [
          createNode({ id: "ok", type: "prose", props: { markdown: "hi" } }),
          createNode({ id: "bad", type: "prose" }), // missing required prop
        ],
      },
    });
    const issues = validateTree(root, reg);
    expect(issues).toHaveLength(1);
    expect(issues[0]?.nodeId).toBe("bad");
  });
});
