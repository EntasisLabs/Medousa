import { describe, expect, it } from "vitest";
import type { ComponentDef, SurfaceDef } from "$lib/types/environment";
import {
  mainComponentsForSurface,
  resolveLayoutRoot,
  shouldFillMainComponent,
} from "$lib/utils/layoutPresentation";

const surface = (overrides: Partial<SurfaceDef> = {}): SurfaceDef => ({
  id: "adhd-guide",
  label: "ADHD Guide",
  icon: "brain",
  kind: "custom",
  layout: "dashboard",
  slots: [],
  ...overrides,
});

const component = (id: string): ComponentDef => ({
  id,
  type: "presentation",
  surfaceId: "adhd-guide",
  slot: "main",
  label: id,
  config: { artifactId: "art-demo" },
  presentation: "panel",
  feeds: [],
});

describe("layoutPresentation", () => {
  it("builds implicit vstack when layoutRoot is absent", () => {
    const root = resolveLayoutRoot(surface(), [component("a"), component("b")]);
    expect(root.type).toBe("vstack");
    if (root.type === "vstack") {
      expect(root.children).toHaveLength(2);
      expect(root.children[0]?.type).toBe("component");
    }
  });

  it("uses explicit layoutRoot when present", () => {
    const root = resolveLayoutRoot(
      surface({
        layoutRoot: {
          type: "hstack",
          spacing: "md",
          distribution: "fill_equally",
          children: [
            { type: "component", id: "a", flex: 1 },
            { type: "component", id: "b", flex: 1 },
          ],
        },
      }),
      [component("a"), component("b")],
    );
    expect(root.type).toBe("hstack");
  });

  it("fills dashboard components in fill_equally hstack", () => {
    expect(
      shouldFillMainComponent({
        surfaceLayout: "dashboard",
        parentType: "hstack",
        siblingCount: 2,
        distribution: "fill_equally",
        flex: 1,
      }),
    ).toBe(true);
  });

  it("filters main components for a surface", () => {
    const components = [
      component("a"),
      { ...component("b"), slot: "header" },
      { ...component("c"), surfaceId: "other" },
    ];
    expect(mainComponentsForSurface("adhd-guide", components).map((c) => c.id)).toEqual(["a"]);
  });

  it("normalizes h_stack and fillEqually aliases from models", () => {
    const root = resolveLayoutRoot(
      surface({
        layoutRoot: {
          type: "h_stack" as "hstack",
          spacing: "md",
          distribution: "fillEqually" as "fill_equally",
          children: [
            { type: "component", id: "a", flex: 1 },
            { type: "component", id: "b", flex: 1 },
          ],
        },
      }),
      [component("a"), component("b")],
    );
    expect(root.type).toBe("hstack");
    if (root.type === "hstack") {
      expect(root.distribution).toBe("fill_equally");
    }
  });
});
