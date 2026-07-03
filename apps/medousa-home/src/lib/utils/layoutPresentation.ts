import type {
  ComponentDef,
  LayoutNode,
  StackAlign,
  StackDistribution,
  StackSpacing,
  SurfaceDef,
  SurfaceLayout,
} from "$lib/types/environment";

export interface LayoutFillContext {
  surfaceLayout: SurfaceLayout | undefined;
  parentType: "vstack" | "hstack" | "grid" | null;
  siblingCount: number;
  distribution?: StackDistribution;
  flex?: number | null;
}

export function spacingToGap(spacing: StackSpacing | undefined): string {
  switch (spacing) {
    case "sm":
      return "0.375rem";
    case "md":
      return "0.75rem";
    case "lg":
      return "1.25rem";
    default:
      return "0";
  }
}

export function alignToCss(align: StackAlign | undefined, direction: "row" | "column"): string {
  const cross = direction === "column" ? "horizontal" : "vertical";
  const value = align ?? "start";
  if (cross === "horizontal") {
    switch (value) {
      case "center":
        return "center";
      case "end":
        return "flex-end";
      case "stretch":
        return "stretch";
      default:
        return "flex-start";
    }
  }
  switch (value) {
    case "center":
      return "center";
    case "end":
      return "flex-end";
    case "stretch":
      return "stretch";
    default:
      return "flex-start";
  }
}

export function stackCrossAlign(
  align: StackAlign | undefined,
  direction: "row" | "column",
  options?: { dashboard?: boolean; distribution?: StackDistribution },
): string {
  if (options?.dashboard) {
    if (direction === "row") {
      return "stretch";
    }
    if (options.distribution === "fill_equally") {
      return "stretch";
    }
  }
  return alignToCss(align, direction);
}

export function distributionToJustify(distribution: StackDistribution | undefined): string {
  switch (distribution) {
    case "center":
      return "center";
    case "end":
      return "flex-end";
    case "space_between":
      return "space-between";
    case "fill_equally":
      return "flex-start";
    default:
      return "flex-start";
  }
}

export function flexStyle(flex: number | null | undefined, distribution?: StackDistribution): string | undefined {
  if (flex != null && flex > 0) {
    return `${flex} 1 0%`;
  }
  if (distribution === "fill_equally") {
    return "1 1 0%";
  }
  return undefined;
}

export function shouldFillMainComponent(ctx: LayoutFillContext): boolean {
  if (ctx.surfaceLayout !== "dashboard") return false;
  if (ctx.flex != null && ctx.flex >= 1) return true;
  if (ctx.distribution === "fill_equally") return true;
  if (ctx.parentType === "vstack" && ctx.siblingCount === 1) return true;
  if (ctx.parentType === "hstack" && ctx.distribution === "fill_equally") return true;
  return false;
}

export function mainComponentsForSurface(
  surfaceId: string,
  components: ComponentDef[],
): ComponentDef[] {
  return components.filter(
    (component) => component.surfaceId === surfaceId && component.slot === "main",
  );
}

export function normalizeStackSpacing(value: unknown): StackSpacing | undefined {
  if (typeof value !== "string") return undefined;
  const key = value.trim().toLowerCase().replace(/-/g, "_");
  if (key === "none" || key === "sm" || key === "md" || key === "lg") return key;
  return undefined;
}

export function normalizeStackDistribution(value: unknown): StackDistribution | undefined {
  if (typeof value !== "string") return undefined;
  const key = value
    .trim()
    .replace(/([A-Z])/g, "_$1")
    .toLowerCase()
    .replace(/^_/, "")
    .replace(/-/g, "_");
  if (
    key === "start" ||
    key === "center" ||
    key === "end" ||
    key === "space_between" ||
    key === "fill_equally"
  ) {
    return key;
  }
  return undefined;
}

export function normalizeLayoutNodeType(value: unknown): LayoutNode["type"] | null {
  if (typeof value !== "string") return null;
  const key = value
    .trim()
    .replace(/([A-Z])/g, "_$1")
    .toLowerCase()
    .replace(/^_/, "")
    .replace(/-/g, "_");
  if (key === "vstack" || key === "v_stack") return "vstack";
  if (key === "hstack" || key === "h_stack") return "hstack";
  if (key === "grid") return "grid";
  if (key === "component") return "component";
  if (key === "slot") return "slot";
  return null;
}

/** Accept model-friendly aliases (`h_stack`, `fillEqually`, …) when reading specs from the daemon. */
export function normalizeLayoutNode(node: LayoutNode): LayoutNode {
  const type = normalizeLayoutNodeType(node.type) ?? node.type;
  if (type === "component") {
    if (node.type !== "component") return node;
    return node;
  }
  if (type === "slot") {
    if (node.type !== "slot") return node;
    return node;
  }
  if (type === "grid") {
    if (node.type !== "grid") return node;
    return {
      ...node,
      spacing: normalizeStackSpacing(node.spacing) ?? node.spacing,
      children: node.children.map(normalizeLayoutNode),
    };
  }
  if (type === "vstack" || type === "hstack") {
    const stack = node as Extract<LayoutNode, { type: "vstack" | "hstack" }>;
    const normalized = {
      type,
      spacing: normalizeStackSpacing(stack.spacing) ?? stack.spacing,
      align: stack.align,
      distribution: normalizeStackDistribution(stack.distribution) ?? stack.distribution,
      children: stack.children.map(normalizeLayoutNode),
    };
    return normalized as LayoutNode;
  }
  return node;
}

export function resolveLayoutRoot(surface: SurfaceDef, components: ComponentDef[]): LayoutNode {
  if (surface.layoutRoot) {
    return normalizeLayoutNode(surface.layoutRoot);
  }
  const mains = mainComponentsForSurface(surface.id, components);
  const distribution =
    surface.layout === "dashboard" && mains.length > 1 ? "fill_equally" : "start";
  return {
    type: "vstack",
    spacing: "md",
    align: "start",
    distribution,
    children: mains.map((component) => ({
      type: "component",
      id: component.id,
    })),
  };
}

export function componentById(
  components: ComponentDef[],
  componentId: string,
): ComponentDef | null {
  return components.find((component) => component.id === componentId) ?? null;
}
