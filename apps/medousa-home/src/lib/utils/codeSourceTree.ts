import type { ForgeSourceTree } from "$lib/forge";

export type CodeSourceTreeNode = {
  kind: "directory" | "file";
  name: string;
  path: string;
  byteSize?: number;
  status?: string | null;
  children: CodeSourceTreeNode[];
};

export type CodeSourceTreeRow = CodeSourceTreeNode & { depth: number };

const YIELD_EVERY = 400;

function yieldToMain(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === "function") {
      requestAnimationFrame(() => resolve());
      return;
    }
    setTimeout(resolve, 0);
  });
}

function compareNodes(left: CodeSourceTreeNode, right: CodeSourceTreeNode): number {
  if (left.kind !== right.kind) return left.kind === "directory" ? -1 : 1;
  // Avoid localeCompare({ numeric: true }) — it is far more expensive on large trees.
  if (left.name < right.name) return -1;
  if (left.name > right.name) return 1;
  return 0;
}

function sortNodes(nodes: CodeSourceTreeNode[]) {
  nodes.sort(compareNodes);
  for (const node of nodes) {
    if (node.children.length > 1) sortNodes(node.children);
  }
}

/** Sync build — prefer {@link buildCodeSourceTreeAsync} on the UI thread. */
export function buildCodeSourceTree(
  files: ForgeSourceTree["files"],
): CodeSourceTreeNode[] {
  const roots: CodeSourceTreeNode[] = [];
  const directories = new Map<string, CodeSourceTreeNode>();
  for (const file of files) {
    const parts = file.path.split("/").filter(Boolean);
    if (parts.length === 0) continue;
    let parent = roots;
    let prefix = "";
    for (const part of parts.slice(0, -1)) {
      prefix = prefix ? `${prefix}/${part}` : part;
      let directory = directories.get(prefix);
      if (!directory) {
        directory = {
          kind: "directory",
          name: part,
          path: prefix,
          children: [],
        };
        directories.set(prefix, directory);
        parent.push(directory);
      }
      parent = directory.children;
    }
    parent.push({
      kind: "file",
      name: parts.at(-1)!,
      path: file.path,
      byteSize: file.byte_size,
      status: file.status,
      children: [],
    });
  }
  sortNodes(roots);
  return roots;
}

/**
 * Build the nested explorer tree without monopolizing the UI thread.
 * Yields to the event loop every {@link YIELD_EVERY} files so clicks/tabs stay responsive.
 */
export async function buildCodeSourceTreeAsync(
  files: ForgeSourceTree["files"],
  isCancelled?: () => boolean,
): Promise<CodeSourceTreeNode[]> {
  const roots: CodeSourceTreeNode[] = [];
  const directories = new Map<string, CodeSourceTreeNode>();
  for (let index = 0; index < files.length; index += 1) {
    if (index > 0 && index % YIELD_EVERY === 0) {
      await yieldToMain();
      if (isCancelled?.()) return roots;
    }
    const file = files[index]!;
    const parts = file.path.split("/").filter(Boolean);
    if (parts.length === 0) continue;
    let parent = roots;
    let prefix = "";
    for (const part of parts.slice(0, -1)) {
      prefix = prefix ? `${prefix}/${part}` : part;
      let directory = directories.get(prefix);
      if (!directory) {
        directory = {
          kind: "directory",
          name: part,
          path: prefix,
          children: [],
        };
        directories.set(prefix, directory);
        parent.push(directory);
      }
      parent = directory.children;
    }
    parent.push({
      kind: "file",
      name: parts.at(-1)!,
      path: file.path,
      byteSize: file.byte_size,
      status: file.status,
      children: [],
    });
  }
  await yieldToMain();
  if (isCancelled?.()) return roots;
  sortNodes(roots);
  return roots;
}

export function flattenCodeSourceTree(
  nodes: CodeSourceTreeNode[],
  expanded: ReadonlySet<string>,
  depth = 0,
): CodeSourceTreeRow[] {
  const rows: CodeSourceTreeRow[] = [];
  for (const node of nodes) {
    rows.push({ ...node, depth });
    if (node.kind === "directory" && expanded.has(node.path)) {
      const nested = flattenCodeSourceTree(node.children, expanded, depth + 1);
      for (const row of nested) rows.push(row);
    }
  }
  return rows;
}

/** Hard cap on painted rows so a wide expand cannot freeze the shell. */
export const CODE_TREE_MAX_VISIBLE_ROWS = 400;
