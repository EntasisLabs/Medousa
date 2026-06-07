import type { VaultNote, VaultTreeNode } from "$lib/types/vault";

export function buildVaultTree(notes: VaultNote[]): VaultTreeNode[] {
  const root: VaultTreeNode = {
    name: "",
    path: null,
    children: [],
    isFolder: true,
  };

  for (const note of notes) {
    const parts = note.path.split("/").filter(Boolean);
    let current = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isLeaf = i === parts.length - 1;
      let child = current.children.find((node) => node.name === part);
      if (!child) {
        child = {
          name: part,
          path: isLeaf ? note.path : null,
          children: [],
          isFolder: !isLeaf,
        };
        current.children.push(child);
      } else if (isLeaf) {
        child.path = note.path;
        child.isFolder = false;
      }
      current = child;
    }
  }

  sortTree(root);
  return root.children;
}

function sortTree(node: VaultTreeNode) {
  node.children.sort((a, b) => {
    if (a.isFolder !== b.isFolder) return a.isFolder ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
  for (const child of node.children) {
    sortTree(child);
  }
}
