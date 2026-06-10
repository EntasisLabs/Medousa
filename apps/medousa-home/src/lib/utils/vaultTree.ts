import type { VaultNote, VaultTreeNode } from "$lib/types/vault";
import {
  VAULT_OTHER_SPACE,
  VAULT_SPACES,
  VAULT_SYSTEM_BUCKET,
  isSystemNoiseNote,
  resolveSpaceForPath,
} from "$lib/config/vaultSpaces";

export interface BuildVaultTreeOptions {
  showSystemNotes: boolean;
  /** When set, only render this space root (M7b). */
  spaceFilter?: string | null;
  /** M7f: only notes the agent touched in the last 24h. */
  agentReviewOnly?: boolean;
  agentWrittenAt?: Record<string, string>;
}

const AGENT_WRITE_TTL_MS = 24 * 60 * 60 * 1000;

function isRecentAgentWrite(
  path: string,
  agentWrittenAt: Record<string, string>,
): boolean {
  const writtenAt = agentWrittenAt[path];
  if (!writtenAt) return false;
  return Date.now() - Date.parse(writtenAt) < AGENT_WRITE_TTL_MS;
}

export function buildVaultTree(
  notes: VaultNote[],
  options: BuildVaultTreeOptions,
): VaultTreeNode[] {
  const visible = notes.filter((note) => {
    if (options.agentReviewOnly) {
      const map = options.agentWrittenAt ?? {};
      if (!isRecentAgentWrite(note.path, map)) return false;
    }
    if (options.showSystemNotes) return true;
    return !isSystemNoiseNote(note.path, note.title);
  });

  const buckets = new Map<string, VaultNote[]>();
  for (const space of [...VAULT_SPACES, VAULT_OTHER_SPACE, VAULT_SYSTEM_BUCKET]) {
    buckets.set(space.id, []);
  }

  for (const note of visible) {
    const space = resolveSpaceForPath(note.path, note.title);
    const bucket = buckets.get(space.id) ?? buckets.get(VAULT_OTHER_SPACE.id)!;
    bucket.push(note);
  }

  const spaceOrder = [...VAULT_SPACES, VAULT_OTHER_SPACE, VAULT_SYSTEM_BUCKET].sort(
    (a, b) => a.sort - b.sort,
  );

  const roots: VaultTreeNode[] = [];
  for (const space of spaceOrder) {
    if (options.spaceFilter && space.id !== options.spaceFilter) continue;
    const bucket = buckets.get(space.id) ?? [];
    if (bucket.length === 0 && !space.alwaysShow) continue;
    if (
      space.id === VAULT_SYSTEM_BUCKET.id &&
      bucket.length === 0 &&
      !options.showSystemNotes
    ) {
      continue;
    }

    const prefix = space.prefix;
    roots.push({
      name: space.id,
      path: null,
      displayLabel: space.label,
      children: buildPathTree(bucket, prefix, space.id),
      isFolder: true,
      spaceId: space.id,
      defaultCollapsed: space.defaultCollapsed ?? false,
      noteCount: bucket.length,
    });
  }

  return roots;
}

function relativePathParts(path: string, prefix: string, spaceId: string): string[] {
  if (spaceId === VAULT_OTHER_SPACE.id || spaceId === VAULT_SYSTEM_BUCKET.id) {
    return path.split("/").filter(Boolean);
  }
  if (prefix && path.startsWith(prefix)) {
    return path.slice(prefix.length).split("/").filter(Boolean);
  }
  return path.split("/").filter(Boolean);
}

function buildPathTree(
  notes: VaultNote[],
  prefix: string,
  spaceId: string,
): VaultTreeNode[] {
  const root: VaultTreeNode = {
    name: "",
    path: null,
    children: [],
    isFolder: true,
  };

  for (const note of notes) {
    const parts = relativePathParts(note.path, prefix, spaceId);
    let current = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isLeaf = i === parts.length - 1;
      let child = current.children.find((node) => node.name === part);
      if (!child) {
        child = {
          name: part,
          path: isLeaf ? note.path : null,
          title: isLeaf ? note.title : null,
          kind: isLeaf ? note.kind ?? null : null,
          children: [],
          isFolder: !isLeaf,
        };
        current.children.push(child);
      } else if (isLeaf) {
        child.path = note.path;
        child.title = note.title;
        child.kind = note.kind ?? null;
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
