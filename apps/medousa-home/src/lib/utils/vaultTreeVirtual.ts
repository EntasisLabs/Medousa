import type { VaultTreeNode } from "$lib/types/vault";

export type VaultTreeFlatRow = {
  id: string;
  node: VaultTreeNode;
  depth: number;
};

/**
 * Flatten only expanded visible rows for virtualized rendering (H07.5).
 * Collapsed subtrees contribute no recursive mount work.
 */
export function flattenExpandedTreeRows(
  roots: VaultTreeNode[],
  isExpanded: (expandKey: string) => boolean,
  expandKeyFor: (node: VaultTreeNode) => string,
): VaultTreeFlatRow[] {
  const rows: VaultTreeFlatRow[] = [];
  const walk = (nodes: VaultTreeNode[], depth: number) => {
    for (const node of nodes) {
      const id = `${node.path ?? node.dropPrefix ?? node.name}:${depth}:${node.isFolder ? "f" : "n"}`;
      rows.push({ id, node, depth });
      if (!node.isFolder || node.children.length === 0) continue;
      if (!isExpanded(expandKeyFor(node))) continue;
      walk(node.children, depth + 1);
    }
  };
  walk(roots, 0);
  return rows;
}

export function visibleWindow(
  rowCount: number,
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  overscan: number,
): { start: number; end: number; offsetY: number; totalHeight: number } {
  const totalHeight = rowCount * rowHeight;
  if (rowCount === 0 || viewportHeight <= 0 || rowHeight <= 0) {
    return { start: 0, end: 0, offsetY: 0, totalHeight };
  }
  const first = Math.floor(scrollTop / rowHeight);
  const visible = Math.ceil(viewportHeight / rowHeight) + 1;
  const start = Math.max(0, first - overscan);
  const end = Math.min(rowCount, first + visible + overscan);
  return {
    start,
    end,
    offsetY: start * rowHeight,
    totalHeight,
  };
}
