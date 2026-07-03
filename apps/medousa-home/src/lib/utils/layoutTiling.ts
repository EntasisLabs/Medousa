import type { LayoutNode } from "$lib/types/environment";
import { cloneLayoutNode } from "$lib/utils/layoutEditOps";

export interface TilingPane {
  id: string;
  componentId: string | null;
}

export type TilingNode =
  | { kind: "pane"; pane: TilingPane }
  | { kind: "split"; direction: "horizontal" | "vertical"; first: TilingNode; second: TilingNode };

let paneSeq = 0;

export function newPaneId(): string {
  paneSeq += 1;
  return `pane-${paneSeq}`;
}

export function cloneTilingNode(node: TilingNode): TilingNode {
  if (node.kind === "pane") {
    return { kind: "pane", pane: { ...node.pane } };
  }
  return {
    kind: "split",
    direction: node.direction,
    first: cloneTilingNode(node.first),
    second: cloneTilingNode(node.second),
  };
}

export function collectComponentIds(node: TilingNode, ids: string[] = []): string[] {
  if (node.kind === "pane") {
    if (node.pane.componentId && !ids.includes(node.pane.componentId)) {
      ids.push(node.pane.componentId);
    }
    return ids;
  }
  collectComponentIds(node.first, ids);
  collectComponentIds(node.second, ids);
  return ids;
}

export function findPane(node: TilingNode, paneId: string): TilingPane | null {
  if (node.kind === "pane") {
    return node.pane.id === paneId ? node.pane : null;
  }
  return findPane(node.first, paneId) ?? findPane(node.second, paneId);
}

export function findPaneByComponent(node: TilingNode, componentId: string): TilingPane | null {
  if (node.kind === "pane") {
    return node.pane.componentId === componentId ? node.pane : null;
  }
  return findPaneByComponent(node.first, componentId) ?? findPaneByComponent(node.second, componentId);
}

function mapTiling(node: TilingNode, paneId: string, fn: (pane: TilingPane) => TilingPane): TilingNode {
  if (node.kind === "pane") {
    if (node.pane.id !== paneId) return node;
    return { kind: "pane", pane: fn(node.pane) };
  }
  const first = mapTiling(node.first, paneId, fn);
  const second = mapTiling(node.second, paneId, fn);
  if (first === node.first && second === node.second) return node;
  return { ...node, first, second };
}

function replacePane(node: TilingNode, paneId: string, replacement: TilingNode): TilingNode {
  if (node.kind === "pane") {
    return node.pane.id === paneId ? replacement : node;
  }
  const first = replacePane(node.first, paneId, replacement);
  const second = replacePane(node.second, paneId, replacement);
  if (first === node.first && second === node.second) return node;
  return { ...node, first, second };
}

export function defaultTilingTree(componentIds: string[]): TilingNode {
  if (componentIds.length === 0) {
    return { kind: "pane", pane: { id: newPaneId(), componentId: null } };
  }
  if (componentIds.length === 1) {
    return { kind: "pane", pane: { id: newPaneId(), componentId: componentIds[0] ?? null } };
  }
  if (componentIds.length === 2) {
    return {
      kind: "split",
      direction: "horizontal",
      first: { kind: "pane", pane: { id: newPaneId(), componentId: componentIds[0] ?? null } },
      second: { kind: "pane", pane: { id: newPaneId(), componentId: componentIds[1] ?? null } },
    };
  }

  const left: TilingNode = {
    kind: "pane",
    pane: { id: newPaneId(), componentId: componentIds[0] ?? null },
  };
  let right: TilingNode = {
    kind: "pane",
    pane: { id: newPaneId(), componentId: componentIds[1] ?? null },
  };
  for (let index = 2; index < componentIds.length; index += 1) {
    right = {
      kind: "split",
      direction: "vertical",
      first: right,
      second: { kind: "pane", pane: { id: newPaneId(), componentId: componentIds[index] ?? null } },
    };
  }
  return { kind: "split", direction: "horizontal", first: left, second: right };
}

function layoutChildToTiling(node: LayoutNode): TilingNode | null {
  if (node.type === "component") {
    return { kind: "pane", pane: { id: newPaneId(), componentId: node.id } };
  }
  if (node.type === "slot") {
    return {
      kind: "pane",
      pane: { id: node.id.startsWith("pane-") ? node.id : newPaneId(), componentId: null },
    };
  }
  if (node.type === "hstack" && node.children.length >= 1) {
    let tree = layoutChildToTiling(node.children[0]);
    if (!tree) return null;
    for (let index = 1; index < node.children.length; index += 1) {
      const next = layoutChildToTiling(node.children[index]);
      if (!next) continue;
      tree = { kind: "split", direction: "horizontal", first: tree, second: next };
    }
    return tree;
  }
  if (node.type === "vstack" && node.children.length >= 1) {
    let tree = layoutChildToTiling(node.children[0]);
    if (!tree) return null;
    for (let index = 1; index < node.children.length; index += 1) {
      const next = layoutChildToTiling(node.children[index]);
      if (!next) continue;
      tree = { kind: "split", direction: "vertical", first: tree, second: next };
    }
    return tree;
  }
  return null;
}

function findFirstEmptyPane(node: TilingNode): string | null {
  if (node.kind === "pane") {
    return node.pane.componentId ? null : node.pane.id;
  }
  return findFirstEmptyPane(node.first) ?? findFirstEmptyPane(node.second);
}

function appendPane(node: TilingNode, componentId: string): TilingNode {
  const pane: TilingNode = { kind: "pane", pane: { id: newPaneId(), componentId } };
  if (node.kind === "pane") {
    return { kind: "split", direction: "vertical", first: node, second: pane };
  }
  return { ...node, second: appendPane(node.second, componentId) };
}

export function ensureComponentsPresent(tree: TilingNode, componentIds: string[]): TilingNode {
  const assigned = new Set(collectComponentIds(tree));
  const missing = componentIds.filter((id) => !assigned.has(id));
  if (missing.length === 0) return tree;

  let next = cloneTilingNode(tree);
  for (const componentId of missing) {
    const emptyPaneId = findFirstEmptyPane(next);
    if (emptyPaneId) {
      next = mapTiling(next, emptyPaneId, (pane) => ({ ...pane, componentId }));
      continue;
    }
    next = appendPane(next, componentId);
  }
  return next;
}

export function layoutRootToTiling(root: LayoutNode | null, componentIds: string[]): TilingNode {
  if (!root) return defaultTilingTree(componentIds);
  const tree = layoutChildToTiling(root);
  if (!tree) return defaultTilingTree(componentIds);
  return ensureComponentsPresent(tree, componentIds);
}

function collapseSplit(node: TilingNode): TilingNode {
  if (node.kind === "pane") return node;
  const first = collapseSplit(node.first);
  const second = collapseSplit(node.second);
  if (first.kind === "pane" && !first.pane.componentId && second.kind === "pane") return second;
  if (second.kind === "pane" && !second.pane.componentId && first.kind === "pane") return first;
  return { ...node, first, second };
}

function tilingLeafToLayout(node: TilingNode): LayoutNode | null {
  if (node.kind === "pane") {
    if (!node.pane.componentId) return null;
    return { type: "component", id: node.pane.componentId, flex: 1 };
  }
  const first = tilingLeafToLayout(node.first);
  const second = tilingLeafToLayout(node.second);
  const children = [first, second].filter((child): child is LayoutNode => child !== null);
  if (children.length === 0) return null;
  if (children.length === 1) return children[0];
  return {
    type: node.direction === "horizontal" ? "hstack" : "vstack",
    spacing: "none",
    distribution: "fill_equally",
    align: "stretch",
    children,
  };
}

export function tilingToLayoutRoot(node: TilingNode): LayoutNode {
  const layout = tilingLeafToLayout(collapseSplit(node));
  if (layout) return cloneLayoutNode(layout);
  return {
    type: "vstack",
    spacing: "none",
    distribution: "fill_equally",
    align: "stretch",
    children: [],
  };
}

export function splitPane(
  node: TilingNode,
  paneId: string,
  direction: "horizontal" | "vertical",
): TilingNode {
  const target = findPane(node, paneId);
  if (!target) return node;
  const replacement: TilingNode = {
    kind: "split",
    direction,
    first: { kind: "pane", pane: { ...target } },
    second: { kind: "pane", pane: { id: newPaneId(), componentId: null } },
  };
  return replacePane(node, paneId, replacement);
}

function replaceSplitContainingLeaf(
  node: TilingNode,
  leafPaneId: string,
  replacement: TilingNode,
): TilingNode {
  if (node.kind === "pane") return node;
  if (
    (node.first.kind === "pane" && node.first.pane.id === leafPaneId) ||
    (node.second.kind === "pane" && node.second.pane.id === leafPaneId)
  ) {
    return replacement;
  }
  const first = replaceSplitContainingLeaf(node.first, leafPaneId, replacement);
  const second = replaceSplitContainingLeaf(node.second, leafPaneId, replacement);
  if (first === node.first && second === node.second) return node;
  return { ...node, first, second };
}

function isDirectSplitLeaf(node: TilingNode, paneId: string): boolean {
  if (node.kind === "pane") return false;
  return (
    (node.first.kind === "pane" && node.first.pane.id === paneId) ||
    (node.second.kind === "pane" && node.second.pane.id === paneId)
  );
}

export function canMergePane(node: TilingNode, paneId: string): boolean {
  if (node.kind === "pane") return false;
  if (isDirectSplitLeaf(node, paneId)) return true;
  return canMergePane(node.first, paneId) || canMergePane(node.second, paneId);
}

export function mergePane(node: TilingNode, paneId: string): TilingNode {
  const selected = findPane(node, paneId);
  if (!selected) return node;
  return replaceSplitContainingLeaf(node, paneId, { kind: "pane", pane: { ...selected } });
}

export function moveComponentToPane(
  node: TilingNode,
  componentId: string,
  targetPaneId: string,
): TilingNode {
  const source = findPaneByComponent(node, componentId);
  const target = findPane(node, targetPaneId);
  if (!source || !target || source.id === target.id) return node;

  const sourceComponentId = source.componentId;
  const targetComponentId = target.componentId;
  if (!sourceComponentId) return node;

  let next = node;
  if (targetComponentId) {
    next = mapTiling(next, source.id, (pane) => ({ ...pane, componentId: targetComponentId }));
    next = mapTiling(next, target.id, (pane) => ({ ...pane, componentId: sourceComponentId }));
  } else {
    next = mapTiling(next, source.id, (pane) => ({ ...pane, componentId: null }));
    next = mapTiling(next, target.id, (pane) => ({ ...pane, componentId: sourceComponentId }));
  }
  return next;
}

export function assignComponentToPane(
  node: TilingNode,
  paneId: string,
  componentId: string,
): TilingNode {
  const existing = findPaneByComponent(node, componentId);
  let next = node;
  if (existing && existing.id !== paneId) {
    next = mapTiling(next, existing.id, (pane) => ({ ...pane, componentId: null }));
  }
  next = mapTiling(next, paneId, (pane) => ({ ...pane, componentId }));
  return next;
}

export function clearPaneComponent(node: TilingNode, paneId: string): TilingNode {
  return mapTiling(node, paneId, (pane) => ({ ...pane, componentId: null }));
}

export function firstEmptyPaneId(node: TilingNode): string | null {
  return findFirstEmptyPane(node);
}

export function firstPaneId(node: TilingNode): string {
  if (node.kind === "pane") return node.pane.id;
  return firstPaneId(node.first);
}
