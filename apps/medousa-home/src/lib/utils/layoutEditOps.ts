import type { LayoutNode } from "$lib/types/environment";

export const MAX_LAYOUT_DEPTH = 8;
export const MAX_LAYOUT_NODES = 32;
export const MAX_COMPONENT_FLEX = 8;

export function cloneLayoutNode(node: LayoutNode): LayoutNode {
  if (node.type === "component" || node.type === "slot") {
    return { ...node };
  }
  return {
    ...node,
    children: node.children.map(cloneLayoutNode),
  } as LayoutNode;
}

export function countLayoutNodes(node: LayoutNode): number {
  if (node.type === "component" || node.type === "slot") return 1;
  return 1 + node.children.reduce((sum, child) => sum + countLayoutNodes(child), 0);
}

export function maxLayoutDepth(node: LayoutNode, depth = 1): number {
  if (node.type === "component" || node.type === "slot") return depth;
  return Math.max(...node.children.map((child) => maxLayoutDepth(child, depth + 1)), depth);
}

export function validateLayoutClient(node: LayoutNode): string[] {
  const errors: string[] = [];
  if (countLayoutNodes(node) > MAX_LAYOUT_NODES) {
    errors.push(`layout exceeds max nodes (${MAX_LAYOUT_NODES})`);
  }
  if (maxLayoutDepth(node) > MAX_LAYOUT_DEPTH) {
    errors.push(`layout exceeds max depth (${MAX_LAYOUT_DEPTH})`);
  }
  return errors;
}

export function nextSlotId(root: LayoutNode): string {
  const ids = new Set<string>();
  walkLayout(root, (node) => {
    if (node.type === "slot") ids.add(node.id);
  });
  let index = 1;
  while (ids.has(`zone-${index}`)) index += 1;
  return `zone-${index}`;
}

function walkLayout(node: LayoutNode, visit: (node: LayoutNode) => void): void {
  visit(node);
  if (node.type === "vstack" || node.type === "hstack" || node.type === "grid") {
    for (const child of node.children) walkLayout(child, visit);
  }
}

export function addSlotNode(root: LayoutNode, slotId?: string): LayoutNode {
  const id = slotId ?? nextSlotId(root);
  const slot: LayoutNode = { type: "slot", id, flex: 1 };
  if (root.type === "vstack" || root.type === "hstack") {
    return { ...root, children: [...root.children, slot] };
  }
  return {
    type: "vstack",
    spacing: "md",
    distribution: "fill_equally",
    children: [root, slot],
  };
}

export function removeSlotNode(root: LayoutNode, slotId: string): LayoutNode | null {
  return pruneNode(root, slotId, "slot");
}

function pruneNode(root: LayoutNode, targetId: string, kind: "slot" | "component"): LayoutNode | null {
  if (root.type === kind && root.id === targetId) return null;
  if (root.type === "component" || root.type === "slot") return root;
  const children = root.children
    .map((child) => pruneNode(child, targetId, kind))
    .filter((child): child is LayoutNode => child !== null);
  if (children.length === 0) return null;
  return { ...root, children };
}

export function assignComponentToSlot(
  root: LayoutNode,
  slotId: string,
  componentId: string,
): LayoutNode {
  return mapLayout(root, (node) => {
    if (node.type === "slot" && node.id === slotId) {
      return { type: "component", id: componentId, flex: node.flex ?? 1 };
    }
    return node;
  });
}

function mapLayout(node: LayoutNode, fn: (node: LayoutNode) => LayoutNode): LayoutNode {
  const mapped = fn(node);
  if (mapped.type === "vstack" || mapped.type === "hstack" || mapped.type === "grid") {
    return {
      ...mapped,
      children: mapped.children.map((child) => mapLayout(child, fn)),
    };
  }
  return mapped;
}

export function splitSelectionHorizontal(root: LayoutNode, componentId: string): LayoutNode {
  return wrapPair(root, componentId, "hstack");
}

export function splitSelectionVertical(root: LayoutNode, componentId: string): LayoutNode {
  return wrapPair(root, componentId, "vstack");
}

function wrapPair(root: LayoutNode, componentId: string, direction: "hstack" | "vstack"): LayoutNode {
  const slot: LayoutNode = { type: "slot", id: nextSlotId(root), flex: 1 };
  const component: LayoutNode = { type: "component", id: componentId, flex: 1 };
  const pair: LayoutNode =
    direction === "hstack"
      ? {
          type: "hstack",
          spacing: "md",
          distribution: "fill_equally",
          children: [component, slot],
        }
      : {
          type: "vstack",
          spacing: "md",
          distribution: "fill_equally",
          children: [component, slot],
        };
  return replaceComponentNode(root, componentId, pair);
}

function replaceComponentNode(root: LayoutNode, componentId: string, replacement: LayoutNode): LayoutNode {
  if (root.type === "component" && root.id === componentId) return replacement;
  if (root.type === "component" || root.type === "slot") return root;
  return {
    ...root,
    children: root.children.map((child) => replaceComponentNode(child, componentId, replacement)),
  };
}

export function moveComponentToSlot(
  root: LayoutNode,
  componentId: string,
  slotId: string,
): LayoutNode {
  let without = removeComponentNode(root, componentId);
  if (!without) return root;
  without = assignComponentToSlot(without, slotId, componentId);
  return without;
}

function removeComponentNode(root: LayoutNode, componentId: string): LayoutNode | null {
  return pruneNode(root, componentId, "component");
}

export function findComponentParentStack(
  root: LayoutNode,
  componentId: string,
): { parent: LayoutNode; index: number } | null {
  if (root.type === "component" || root.type === "slot") return null;
  for (let index = 0; index < root.children.length; index += 1) {
    const child = root.children[index];
    if (child.type === "component" && child.id === componentId) {
      return { parent: root, index };
    }
    const nested = findComponentParentStack(child, componentId);
    if (nested) return nested;
  }
  return null;
}

export function reorderComponent(
  root: LayoutNode,
  componentId: string,
  targetIndex: number,
): LayoutNode {
  const found = findComponentParentStack(root, componentId);
  if (!found || (found.parent.type !== "vstack" && found.parent.type !== "hstack")) {
    return root;
  }
  const parent = found.parent as Extract<LayoutNode, { type: "vstack" | "hstack" }>;
  const children = [...parent.children];
  const [item] = children.splice(found.index, 1);
  children.splice(Math.max(0, Math.min(targetIndex, children.length)), 0, item);
  return replaceNode(root, parent, { ...parent, children });
}

function replaceNode(root: LayoutNode, target: LayoutNode, replacement: LayoutNode): LayoutNode {
  if (root === target) return replacement;
  if (root.type === "component" || root.type === "slot") return root;
  return {
    ...root,
    children: root.children.map((child) => replaceNode(child, target, replacement)),
  };
}
