import type { SplitDirection, SplitNode } from "$lib/types/shellTabs";

export const RATIO_MIN = 0.2;
export const RATIO_MAX = 0.8;
export const RATIO_DEFAULT = 0.5;

export function clampRatio(ratio: number): number {
  if (!Number.isFinite(ratio)) return RATIO_DEFAULT;
  return Math.min(RATIO_MAX, Math.max(RATIO_MIN, ratio));
}

export function newSplitId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

export function countLeaves(node: SplitNode): number {
  if (node.type === "group") return 1;
  return countLeaves(node.a) + countLeaves(node.b);
}

export function collectGroupIds(node: SplitNode): string[] {
  if (node.type === "group") return [node.id];
  return [...collectGroupIds(node.a), ...collectGroupIds(node.b)];
}

export function findGroupLeaf(node: SplitNode, groupId: string): boolean {
  if (node.type === "group") return node.id === groupId;
  return findGroupLeaf(node.a, groupId) || findGroupLeaf(node.b, groupId);
}

/** Split the leaf `groupId` into a branch; returns new root + new group id. */
export function splitLeaf(
  root: SplitNode,
  groupId: string,
  direction: SplitDirection,
  newGroupId: string,
): { root: SplitNode; newGroupId: string } | null {
  const branchDirection = direction === "right" ? "column" : "row";

  function walk(node: SplitNode): SplitNode | null {
    if (node.type === "group") {
      if (node.id !== groupId) return null;
      return {
        type: "branch",
        id: newSplitId("branch"),
        direction: branchDirection,
        ratio: RATIO_DEFAULT,
        a: node,
        b: { type: "group", id: newGroupId },
      };
    }
    const nextA = walk(node.a);
    if (nextA) return { ...node, a: nextA };
    const nextB = walk(node.b);
    if (nextB) return { ...node, b: nextB };
    return null;
  }

  const next = walk(root);
  if (!next) return null;
  return { root: next, newGroupId };
}

/**
 * Remove leaf `groupId` and promote its sibling.
 * Returns null if the leaf is the only remaining pane.
 */
export function removeLeaf(
  root: SplitNode,
  groupId: string,
): { root: SplitNode; removed: boolean } {
  if (root.type === "group") {
    return { root, removed: false };
  }

  if (root.a.type === "group" && root.a.id === groupId) {
    return { root: root.b, removed: true };
  }
  if (root.b.type === "group" && root.b.id === groupId) {
    return { root: root.a, removed: true };
  }

  const left = removeLeaf(root.a, groupId);
  if (left.removed) {
    if (left.root.type === "group" || left.root.type === "branch") {
      return { root: { ...root, a: left.root }, removed: true };
    }
  }
  const right = removeLeaf(root.b, groupId);
  if (right.removed) {
    return { root: { ...root, b: right.root }, removed: true };
  }
  return { root, removed: false };
}

export function setBranchRatio(
  root: SplitNode,
  branchId: string,
  ratio: number,
): SplitNode {
  const nextRatio = clampRatio(ratio);
  if (root.type === "group") return root;
  if (root.id === branchId) {
    return { ...root, ratio: nextRatio };
  }
  return {
    ...root,
    a: setBranchRatio(root.a, branchId, nextRatio),
    b: setBranchRatio(root.b, branchId, nextRatio),
  };
}

export type FocusDir = "left" | "right" | "up" | "down";

/** Flat leaf order: depth-first, a then b. */
export function leafOrder(node: SplitNode): string[] {
  return collectGroupIds(node);
}

export function neighborInDirection(
  root: SplitNode,
  groupId: string,
  dir: FocusDir,
): string | null {
  const order = leafOrder(root);
  const idx = order.indexOf(groupId);
  if (idx < 0) return null;

  // Approximate geometry from tree: column → horizontal, row → vertical.
  if (dir === "left" || dir === "right") {
    const step = dir === "left" ? -1 : 1;
    const next = idx + step;
    return order[next] ?? null;
  }
  const step = dir === "up" ? -1 : 1;
  const next = idx + step;
  return order[next] ?? null;
}

export function migrateV1ToSplitRoot(groupId: string): SplitNode {
  return { type: "group", id: groupId };
}
