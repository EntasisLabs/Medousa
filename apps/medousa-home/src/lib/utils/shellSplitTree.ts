import type { SplitDirection, SplitEdge, SplitNode } from "$lib/types/shellTabs";

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

/** Split the leaf so the new group lands on `edge` of the host. */
export function splitLeafAtEdge(
  root: SplitNode,
  groupId: string,
  edge: SplitEdge,
  newGroupId: string,
): { root: SplitNode; newGroupId: string } | null {
  const branchDirection = edge === "left" || edge === "right" ? "column" : "row";
  const newFirst = edge === "left" || edge === "top";
  const newLeaf: SplitNode = { type: "group", id: newGroupId };

  function walk(node: SplitNode): SplitNode | null {
    if (node.type === "group") {
      if (node.id !== groupId) return null;
      return {
        type: "branch",
        id: newSplitId("branch"),
        direction: branchDirection,
        ratio: RATIO_DEFAULT,
        a: newFirst ? newLeaf : node,
        b: newFirst ? node : newLeaf,
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

/** Split the leaf `groupId` into a branch; returns new root + new group id. */
export function splitLeaf(
  root: SplitNode,
  groupId: string,
  direction: SplitDirection,
  newGroupId: string,
): { root: SplitNode; newGroupId: string } | null {
  return splitLeafAtEdge(
    root,
    groupId,
    direction === "right" ? "right" : "bottom",
    newGroupId,
  );
}

/**
 * Leaf group that should receive tabs when `groupId` is closed/merged —
 * the sash-adjacent leaf in the immediate sibling subtree.
 */
export function mergeTargetForLeaf(root: SplitNode, groupId: string): string | null {
  function walk(node: SplitNode): string | null {
    if (node.type === "group") return null;
    if (node.a.type === "group" && node.a.id === groupId) {
      return collectGroupIds(node.b)[0] ?? null;
    }
    if (node.b.type === "group" && node.b.id === groupId) {
      const ids = collectGroupIds(node.a);
      return ids[ids.length - 1] ?? null;
    }
    return walk(node.a) ?? walk(node.b);
  }
  return walk(root);
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

type LayoutRect = { x: number; y: number; w: number; h: number };

/** Unit-square geometry for each leaf from the binary split tree. */
export function leafRects(
  root: SplitNode,
  bounds: LayoutRect = { x: 0, y: 0, w: 1, h: 1 },
): Map<string, LayoutRect> {
  const out = new Map<string, LayoutRect>();
  function walk(node: SplitNode, rect: LayoutRect) {
    if (node.type === "group") {
      out.set(node.id, rect);
      return;
    }
    const ratio = clampRatio(node.ratio);
    if (node.direction === "column") {
      const aw = rect.w * ratio;
      walk(node.a, { x: rect.x, y: rect.y, w: aw, h: rect.h });
      walk(node.b, { x: rect.x + aw, y: rect.y, w: rect.w - aw, h: rect.h });
      return;
    }
    const ah = rect.h * ratio;
    walk(node.a, { x: rect.x, y: rect.y, w: rect.w, h: ah });
    walk(node.b, { x: rect.x, y: rect.y + ah, w: rect.w, h: rect.h - ah });
  }
  walk(root, bounds);
  return out;
}

function overlap1d(a0: number, a1: number, b0: number, b1: number): number {
  return Math.max(0, Math.min(a1, b1) - Math.max(a0, b0));
}

/**
 * Focus neighbor by rendered geometry (unit-square layout), not flat leaf index.
 * Prefers sash-adjacent leaves with edge overlap, then nearest center in-axis.
 */
export function neighborInDirection(
  root: SplitNode,
  groupId: string,
  dir: FocusDir,
): string | null {
  const rects = leafRects(root);
  const current = rects.get(groupId);
  if (!current) return null;

  const cx = current.x + current.w / 2;
  const cy = current.y + current.h / 2;
  const eps = 1e-6;

  type Candidate = { id: string; overlap: number; gap: number; cross: number };
  const candidates: Candidate[] = [];

  for (const [id, rect] of rects) {
    if (id === groupId) continue;
    const rx = rect.x + rect.w / 2;
    const ry = rect.y + rect.h / 2;

    if (dir === "left") {
      if (rx >= cx - eps) continue;
      const gap = current.x - (rect.x + rect.w);
      if (gap < -eps) continue;
      candidates.push({
        id,
        overlap: overlap1d(current.y, current.y + current.h, rect.y, rect.y + rect.h),
        gap: Math.max(0, gap),
        cross: Math.abs(cy - ry),
      });
    } else if (dir === "right") {
      if (rx <= cx + eps) continue;
      const gap = rect.x - (current.x + current.w);
      if (gap < -eps) continue;
      candidates.push({
        id,
        overlap: overlap1d(current.y, current.y + current.h, rect.y, rect.y + rect.h),
        gap: Math.max(0, gap),
        cross: Math.abs(cy - ry),
      });
    } else if (dir === "up") {
      if (ry >= cy - eps) continue;
      const gap = current.y - (rect.y + rect.h);
      if (gap < -eps) continue;
      candidates.push({
        id,
        overlap: overlap1d(current.x, current.x + current.w, rect.x, rect.x + rect.w),
        gap: Math.max(0, gap),
        cross: Math.abs(cx - rx),
      });
    } else {
      if (ry <= cy + eps) continue;
      const gap = rect.y - (current.y + current.h);
      if (gap < -eps) continue;
      candidates.push({
        id,
        overlap: overlap1d(current.x, current.x + current.w, rect.x, rect.x + rect.w),
        gap: Math.max(0, gap),
        cross: Math.abs(cx - rx),
      });
    }
  }

  if (candidates.length === 0) return null;
  candidates.sort((a, b) => {
    if (a.gap !== b.gap) return a.gap - b.gap;
    if (b.overlap !== a.overlap) return b.overlap - a.overlap;
    return a.cross - b.cross;
  });
  return candidates[0]?.id ?? null;
}

export function migrateV1ToSplitRoot(groupId: string): SplitNode {
  return { type: "group", id: groupId };
}
