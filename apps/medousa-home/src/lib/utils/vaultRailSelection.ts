/** Ordered note paths for shift-range selection in the vault rail. */

import type { VaultTreeNode } from "$lib/types/vault";

export function flattenTreeNotePaths(nodes: VaultTreeNode[]): string[] {
  const paths: string[] = [];
  const walk = (list: VaultTreeNode[]) => {
    for (const node of list) {
      if (node.path && !node.isFolder) {
        paths.push(node.path);
      }
      if (node.children.length > 0) {
        walk(node.children);
      }
    }
  };
  walk(nodes);
  return paths;
}

export function rangePathsBetween(
  ordered: string[],
  anchor: string,
  focus: string,
): string[] {
  const start = ordered.indexOf(anchor);
  const end = ordered.indexOf(focus);
  if (start < 0 && end < 0) return [focus];
  if (start < 0) return [focus];
  if (end < 0) return [anchor];
  const lo = Math.min(start, end);
  const hi = Math.max(start, end);
  return ordered.slice(lo, hi + 1);
}
