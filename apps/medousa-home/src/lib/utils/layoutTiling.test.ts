import { describe, expect, it } from "vitest";
import {
  collectComponentIds,
  componentsInReadingOrder,
  layoutRootToTiling,
  tilingToLayoutRoot,
  type TilingNode,
} from "./layoutTiling";
import type { LayoutNode } from "$lib/types/environment";

function pane(componentId: string): TilingNode {
  return { kind: "pane", pane: { id: `pane-${componentId}`, componentId } };
}

function split(
  direction: "horizontal" | "vertical",
  first: TilingNode,
  second: TilingNode,
): TilingNode {
  return { kind: "split", direction, first, second };
}

function componentNode(id: string): LayoutNode {
  return { type: "component", id, flex: 1 };
}

function tilingShape(node: TilingNode): unknown {
  if (node.kind === "pane") {
    return { kind: "pane", componentId: node.pane.componentId };
  }
  return {
    kind: "split",
    direction: node.direction,
    first: tilingShape(node.first),
    second: tilingShape(node.second),
  };
}

describe("layoutTiling roundtrip", () => {
  it("preserves a 2x2 grid through save and reload", () => {
    const tree = split(
      "vertical",
      split("horizontal", pane("a"), pane("b")),
      split("horizontal", pane("c"), pane("d")),
    );

    const saved = tilingToLayoutRoot(tree);
    expect(saved.type).toBe("vstack");
    if (saved.type !== "vstack") return;

    expect(saved.children).toHaveLength(2);
    expect(saved.children[0]?.type).toBe("hstack");
    expect(saved.children[1]?.type).toBe("hstack");

    const restored = layoutRootToTiling(saved, ["a", "b", "c", "d"]);
    expect(collectComponentIds(restored).sort()).toEqual(["a", "b", "c", "d"]);
    expect(tilingShape(restored)).toEqual(tilingShape(tree));
  });

  it("preserves split-left / stacked-right layout", () => {
    const tree = split(
      "horizontal",
      pane("spotify"),
      split("vertical", pane("tetris"), split("vertical", pane("garden"), pane("arcade"))),
    );

    const saved = tilingToLayoutRoot(tree);
    const restored = layoutRootToTiling(saved, ["spotify", "tetris", "garden", "arcade"]);
    expect(tilingShape(restored)).toEqual(tilingShape(tree));
  });

  it("builds nested stacks from flat vstack children", () => {
    const root: LayoutNode = {
      type: "vstack",
      spacing: "none",
      distribution: "fill_equally",
      align: "stretch",
      children: [
        {
          type: "hstack",
          spacing: "none",
          distribution: "fill_equally",
          align: "stretch",
          children: [componentNode("a"), componentNode("b")],
        },
        {
          type: "hstack",
          spacing: "none",
          distribution: "fill_equally",
          align: "stretch",
          children: [componentNode("c"), componentNode("d")],
        },
      ],
    };

    const restored = layoutRootToTiling(root, ["a", "b", "c", "d"]);
    expect(tilingShape(restored)).toEqual(
      tilingShape(
        split(
          "vertical",
          split("horizontal", pane("a"), pane("b")),
          split("horizontal", pane("c"), pane("d")),
        ),
      ),
    );
  });
});

describe("componentsInReadingOrder", () => {
  it("walks left-to-right then top-to-bottom and appends orphans", () => {
    const tree = split(
      "vertical",
      split("horizontal", pane("a"), pane("b")),
      split("horizontal", pane("c"), pane("d")),
    );
    const components = [
      { id: "d" },
      { id: "orphan" },
      { id: "a" },
      { id: "c" },
      { id: "b" },
    ];
    expect(componentsInReadingOrder(tree, components).map((entry) => entry.id)).toEqual([
      "a",
      "b",
      "c",
      "d",
      "orphan",
    ]);
  });
});
