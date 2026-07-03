import type { LayoutNode } from "$lib/types/environment";
import { cloneLayoutNode } from "$lib/utils/layoutEditOps";

export type LayoutTemplateId = "full" | "split" | "stack";

export interface LayoutRegion {
  id: string;
  componentId: string | null;
}

export interface LayoutTemplateState {
  template: LayoutTemplateId;
  regions: LayoutRegion[];
}

function regionId(index: number): string {
  return `region-${index + 1}`;
}

function stackNode(children: LayoutNode[]): LayoutNode {
  return {
    type: "vstack",
    spacing: "none",
    distribution: "fill_equally",
    align: "stretch",
    children,
  };
}

function splitNode(left: LayoutNode, right: LayoutNode): LayoutNode {
  return {
    type: "hstack",
    spacing: "none",
    distribution: "fill_equally",
    align: "stretch",
    children: [left, right],
  };
}

function componentNode(id: string): LayoutNode {
  return { type: "component", id, flex: 1 };
}

function slotNode(id: string): LayoutNode {
  return { type: "slot", id, flex: 1 };
}

function regionLeaf(region: LayoutRegion): LayoutNode | null {
  if (!region.componentId) return null;
  return componentNode(region.componentId);
}

function compactChildren(nodes: Array<LayoutNode | null>): LayoutNode[] {
  return nodes.filter((node): node is LayoutNode => node !== null);
}

function rightSplitColumn(regions: LayoutRegion[]): LayoutNode {
  const assigned = regions.slice(1).filter((region) => region.componentId);
  if (assigned.length === 0) return slotNode(regions[1]?.id ?? "region-2");
  if (assigned.length === 1) return componentNode(assigned[0]!.componentId!);
  return stackNode(assigned.map((region) => componentNode(region.componentId!)));
}

export function defaultTemplateState(componentIds: string[]): LayoutTemplateState {
  if (componentIds.length <= 1) {
    return {
      template: "full",
      regions: [{ id: "region-1", componentId: componentIds[0] ?? null }],
    };
  }
  if (componentIds.length === 2) {
    return {
      template: "split",
      regions: [
        { id: "region-1", componentId: componentIds[0] ?? null },
        { id: "region-2", componentId: componentIds[1] ?? null },
      ],
    };
  }
  return {
    template: "stack",
    regions: componentIds.map((componentId, index) => ({
      id: regionId(index),
      componentId,
    })),
  };
}

export function walkComponentIds(node: LayoutNode, ids: string[] = []): string[] {
  if (node.type === "component") {
    if (!ids.includes(node.id)) ids.push(node.id);
    return ids;
  }
  if (node.type === "vstack" || node.type === "hstack" || node.type === "grid") {
    for (const child of node.children) walkComponentIds(child, ids);
  }
  return ids;
}

export function detectTemplate(root: LayoutNode): LayoutTemplateId {
  if (root.type === "component") return "full";
  if (root.type === "hstack" && root.children.length === 2) return "split";
  if (root.type === "vstack") {
    if (root.children.length === 1) return "full";
    return "stack";
  }
  return "stack";
}

function parseSplitRegions(root: LayoutNode): LayoutRegion[] {
  if (root.type !== "hstack" || root.children.length !== 2) {
    return [
      { id: "region-1", componentId: null },
      { id: "region-2", componentId: null },
    ];
  }
  const [left, right] = root.children;
  const regions: LayoutRegion[] = [
    {
      id: "region-1",
      componentId: left.type === "component" ? left.id : null,
    },
    { id: "region-2", componentId: null },
  ];
  if (right.type === "component") {
    regions[1]!.componentId = right.id;
  } else if (right.type === "slot") {
    regions[1]!.id = right.id.startsWith("region-") ? right.id : "region-2";
  } else if (right.type === "vstack") {
    const nested = right.children
      .filter((child): child is Extract<LayoutNode, { type: "component" }> => child.type === "component")
      .map((child, index) => ({
        id: regionId(index + 1),
        componentId: child.id,
      }));
    if (nested[0]) regions[1] = nested[0]!;
    for (let index = 1; index < nested.length; index += 1) {
      regions.push(nested[index]!);
    }
  }
  return regions;
}

function parseStackRegions(root: LayoutNode): LayoutRegion[] {
  if (root.type === "component") {
    return [{ id: "region-1", componentId: root.id }];
  }
  if (root.type !== "vstack") return [{ id: "region-1", componentId: null }];
  return root.children.map((child, index) => {
    if (child.type === "component") {
      return { id: regionId(index), componentId: child.id };
    }
    if (child.type === "slot") {
      return { id: child.id.startsWith("region-") ? child.id : regionId(index), componentId: null };
    }
    return { id: regionId(index), componentId: null };
  });
}

export function parseTemplateState(root: LayoutNode, componentIds: string[]): LayoutTemplateState {
  const template = detectTemplate(root);
  if (template === "full") {
    const id =
      root.type === "component"
        ? root.id
        : root.type === "vstack" && root.children[0]?.type === "component"
          ? root.children[0].id
          : componentIds[0] ?? null;
    return { template: "full", regions: [{ id: "region-1", componentId: id }] };
  }
  if (template === "split") {
    const regions = parseSplitRegions(root);
    return { template: "split", regions: mergeMissingComponents(regions, componentIds) };
  }
  const regions = parseStackRegions(root);
  return { template: "stack", regions: mergeMissingComponents(regions, componentIds) };
}

function mergeMissingComponents(regions: LayoutRegion[], componentIds: string[]): LayoutRegion[] {
  const assigned = new Set(regions.map((region) => region.componentId).filter(Boolean));
  const next = [...regions];
  for (const componentId of componentIds) {
    if (assigned.has(componentId)) continue;
    const empty = next.find((region) => !region.componentId);
    if (empty) {
      empty.componentId = componentId;
      assigned.add(componentId);
    } else {
      next.push({ id: regionId(next.length), componentId });
    }
  }
  return next;
}

export function normalizeTemplateState(
  root: LayoutNode | null,
  componentIds: string[],
): LayoutTemplateState {
  if (!root || componentIds.length === 0) {
    return defaultTemplateState(componentIds);
  }
  const parsed = parseTemplateState(root, componentIds);
  if (parsed.template === "stack" && componentIds.length >= 3) {
    return {
      template: "stack",
      regions: componentIds.map((componentId, index) => ({
        id: regionId(index),
        componentId,
      })),
    };
  }
  if (componentIds.length === 1) {
    return { template: "full", regions: [{ id: "region-1", componentId: componentIds[0] ?? null }] };
  }
  if (componentIds.length === 2 && parsed.template === "stack") {
    return defaultTemplateState(componentIds);
  }
  return parsed;
}

export function buildLayoutRoot(state: LayoutTemplateState): LayoutNode {
  if (state.template === "full") {
    const region = state.regions[0] ?? { id: "region-1", componentId: null };
    return regionLeaf(region) ?? stackNode([slotNode(region.id)]);
  }

  if (state.template === "split") {
    const left = state.regions[0] ?? { id: "region-1", componentId: null };
    const rightAssigned = state.regions.slice(1).filter((region) => region.componentId);
    const leftNode = regionLeaf(left);
    const rightNodes = compactChildren(rightAssigned.map((region) => regionLeaf(region)!));

    if (leftNode && rightNodes.length === 0) return leftNode;
    if (!leftNode && rightNodes.length === 1) return rightNodes[0]!;
    if (!leftNode && rightNodes.length === 0) return stackNode([slotNode("region-1")]);

    const rightColumn =
      rightNodes.length === 1
        ? rightNodes[0]!
        : stackNode(rightNodes.length ? rightNodes : [slotNode("region-2")]);

    return splitNode(leftNode ?? stackNode([slotNode("region-1")]), rightColumn);
  }

  const rows = compactChildren(
    (state.regions.length ? state.regions : [{ id: "region-1", componentId: null }]).map(
      (region) => regionLeaf(region),
    ),
  );
  if (rows.length === 0) return stackNode([slotNode("region-1")]);
  if (rows.length === 1) return rows[0]!;
  return stackNode(rows);
}

export function applyTemplate(
  template: LayoutTemplateId,
  current: LayoutTemplateState,
  componentIds: string[],
): LayoutTemplateState {
  const ordered = orderComponents(current, componentIds);
  if (template === "full") {
    return {
      template: "full",
      regions: [{ id: "region-1", componentId: ordered[0] ?? null }],
    };
  }
  if (template === "split") {
    return {
      template: "split",
      regions: [
        { id: "region-1", componentId: ordered[0] ?? null },
        { id: "region-2", componentId: ordered[1] ?? null },
        ...ordered.slice(2).map((componentId, index) => ({
          id: regionId(index + 2),
          componentId,
        })),
      ],
    };
  }
  return {
    template: "stack",
    regions: ordered.map((componentId, index) => ({
      id: regionId(index),
      componentId,
    })),
  };
}

function orderComponents(state: LayoutTemplateState, componentIds: string[]): string[] {
  const fromRegions = state.regions
    .map((region) => region.componentId)
    .filter((id): id is string => Boolean(id));
  const seen = new Set(fromRegions);
  const tail = componentIds.filter((id) => !seen.has(id));
  return [...fromRegions, ...tail];
}

export function assignComponentToRegion(
  state: LayoutTemplateState,
  regionId: string,
  componentId: string | null,
): LayoutTemplateState {
  const regions = state.regions.map((region) => ({ ...region }));
  const target = regions.find((region) => region.id === regionId);
  if (!target) return state;

  if (componentId) {
    for (const region of regions) {
      if (region.id !== regionId && region.componentId === componentId) {
        region.componentId = target.componentId;
      }
    }
  }
  target.componentId = componentId;
  return { ...state, regions };
}

export function moveComponentToRegion(
  state: LayoutTemplateState,
  componentId: string,
  targetRegionId: string,
): LayoutTemplateState {
  return assignComponentToRegion(state, targetRegionId, componentId);
}

export function syncTemplateRoot(state: LayoutTemplateState): LayoutNode {
  return cloneLayoutNode(buildLayoutRoot(state));
}

export function regionCountForTemplate(template: LayoutTemplateId, componentCount: number): number {
  if (template === "full") return 1;
  if (template === "split") return Math.max(2, componentCount);
  return Math.max(1, componentCount);
}

export function visibleRegions(state: LayoutTemplateState): LayoutRegion[] {
  if (state.template === "full") {
    return [state.regions[0] ?? { id: "region-1", componentId: null }];
  }
  if (state.template === "split") {
    const left = state.regions[0] ?? { id: "region-1", componentId: null };
    const rightPrimary = state.regions[1] ?? { id: "region-2", componentId: null };
    return [left, rightPrimary];
  }
  return state.regions.length
    ? state.regions
    : [{ id: "region-1", componentId: null }];
}

export function splitRightComponentIds(state: LayoutTemplateState): string[] {
  if (state.template !== "split") return [];
  return state.regions.slice(2).map((region) => region.componentId).filter((id): id is string => Boolean(id));
}

export function templateLabel(template: LayoutTemplateId): string {
  switch (template) {
    case "full":
      return "Full";
    case "split":
      return "Split";
    case "stack":
      return "Stack";
  }
}
