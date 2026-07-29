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
  const sort = (nodes: CodeSourceTreeNode[]) => {
    nodes.sort((left, right) => {
      if (left.kind !== right.kind) return left.kind === "directory" ? -1 : 1;
      return left.name.localeCompare(right.name, undefined, { numeric: true });
    });
    for (const node of nodes) sort(node.children);
  };
  sort(roots);
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
      rows.push(...flattenCodeSourceTree(node.children, expanded, depth + 1));
    }
  }
  return rows;
}
