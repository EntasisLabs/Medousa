import type { VaultNote, VaultNoteSummary } from "$lib/types/vault";

export type WikilinkResolution =
  | { kind: "resolved"; path: string }
  | { kind: "ambiguous"; candidates: string[] }
  | { kind: "missing" };

/**
 * Immutable per-generation vault lookup maps (H07.5).
 * Built once when a daemon generation snapshot/delta is applied.
 */
export interface VaultLookupSnapshot {
  generation: number;
  metadataByPath: Map<string, VaultNoteSummary>;
  knownPaths: Set<string>;
  pathsByStem: Map<string, string[]>;
  pathsByFoldedTitle: Map<string, string[]>;
  parentByNode: Map<string, string | null>;
  ancestorIdsForSelection: Set<string>;
}

function filenameStem(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

function foldedTitle(title: string): string {
  return title.trim().toLowerCase();
}

export function buildVaultLookupSnapshot(
  notes: Array<VaultNote | VaultNoteSummary>,
  generation: number,
  selectedPath: string | null = null,
): VaultLookupSnapshot {
  const metadataByPath = new Map<string, VaultNoteSummary>();
  const knownPaths = new Set<string>();
  const pathsByStem = new Map<string, string[]>();
  const pathsByFoldedTitle = new Map<string, string[]>();
  const parentByNode = new Map<string, string | null>();

  for (const note of notes) {
    const summary: VaultNoteSummary = {
      path: note.path,
      title: note.title,
      modified_at_utc: note.modified_at_utc,
      kind: note.kind,
      tags: note.tags,
    };
    metadataByPath.set(note.path, summary);
    knownPaths.add(note.path);
    const stem = filenameStem(note.path);
    const stemList = pathsByStem.get(stem) ?? [];
    stemList.push(note.path);
    pathsByStem.set(stem, stemList);
    const folded = foldedTitle(note.title);
    if (folded) {
      const titleList = pathsByFoldedTitle.get(folded) ?? [];
      titleList.push(note.path);
      pathsByFoldedTitle.set(folded, titleList);
    }
    const slash = note.path.lastIndexOf("/");
    parentByNode.set(note.path, slash === -1 ? null : note.path.slice(0, slash));
  }

  for (const list of pathsByStem.values()) list.sort();
  for (const list of pathsByFoldedTitle.values()) list.sort();

  return {
    generation,
    metadataByPath,
    knownPaths,
    pathsByStem,
    pathsByFoldedTitle,
    parentByNode,
    ancestorIdsForSelection: ancestorsForPath(selectedPath, parentByNode),
  };
}

export function ancestorsForPath(
  path: string | null,
  parentByNode: Map<string, string | null>,
): Set<string> {
  const ancestors = new Set<string>();
  if (!path) return ancestors;
  let current: string | null = path;
  const seen = new Set<string>();
  while (current) {
    if (seen.has(current)) break;
    seen.add(current);
    ancestors.add(current);
    const parent = parentByNode.get(current);
    if (parent == null) {
      // Also mark folder prefixes for tree nodes without explicit parent map entries.
      const slash = current.lastIndexOf("/");
      current = slash === -1 ? null : current.slice(0, slash);
    } else {
      current = parent;
    }
  }
  // Include every prefix folder of the selected path for O(1) row checks.
  if (path) {
    const parts = path.split("/");
    let prefix = "";
    for (let i = 0; i < parts.length - 1; i += 1) {
      prefix = prefix ? `${prefix}/${parts[i]}` : parts[i]!;
      ancestors.add(prefix);
    }
  }
  return ancestors;
}

export function withSelectionAncestors(
  snapshot: VaultLookupSnapshot,
  selectedPath: string | null,
): VaultLookupSnapshot {
  return {
    ...snapshot,
    ancestorIdsForSelection: ancestorsForPath(selectedPath, snapshot.parentByNode),
  };
}

export function resolveWikilinkWithLookup(
  raw: string,
  sourcePath: string | null,
  lookup: VaultLookupSnapshot,
): WikilinkResolution {
  const token = raw.split("#")[0]?.split("|")[0]?.trim() ?? "";
  if (!token) return { kind: "missing" };

  const candidates: string[] = [];
  const pushUnique = (path: string) => {
    if (!candidates.includes(path)) candidates.push(path);
  };

  if (token.includes("/")) {
    const normalized = token.endsWith(".md") ? token : `${token}.md`;
    pushUnique(normalized);
  } else {
    const stem = filenameStem(token);
    const sourceDir = sourcePath?.includes("/")
      ? sourcePath.slice(0, sourcePath.lastIndexOf("/"))
      : "";
    if (sourceDir) {
      pushUnique(`${sourceDir}/${stem}.md`);
    }
    pushUnique(`${stem}.md`);
    for (const path of lookup.pathsByStem.get(stem) ?? []) {
      pushUnique(path);
    }
    for (const path of lookup.pathsByFoldedTitle.get(stem.toLowerCase()) ?? []) {
      pushUnique(path);
    }
  }

  const hits = candidates.filter((path) => lookup.knownPaths.has(path));
  if (hits.length === 1) return { kind: "resolved", path: hits[0]! };
  if (hits.length > 1) return { kind: "ambiguous", candidates: hits };
  return { kind: "missing" };
}
