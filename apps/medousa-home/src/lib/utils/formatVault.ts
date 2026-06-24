import type { VaultNote } from "$lib/types/vault";

/** Human-facing vault labels — strip hashes everywhere, not just filename tails. */

const HASH_TAIL = /-[0-9a-f]{8,}(?=\.md$)/i;
const EMBEDDED_HEX = /(?:\s+|[-_])(?:[0-9a-f]{8,})\b/gi;
const PURE_HEX = /^[0-9a-f]{20,}$/i;

export function stripEmbeddedHashes(text: string): string {
  return text
    .replace(EMBEDDED_HEX, "")
    .replace(HASH_TAIL, "")
    .replace(/\s{2,}/g, " ")
    .trim();
}

export function vaultDisplayTitle(title: string, path?: string | null): string {
  const trimmed = stripEmbeddedHashes(title.trim());
  if (trimmed && !looksLikeHashSlug(trimmed) && !PURE_HEX.test(trimmed)) {
    return trimmed;
  }
  const fromPath = path ? filenameStem(path) : trimmed;
  return humanizeStem(fromPath);
}

export function vaultDisplayPath(path: string): string {
  return path.replace(HASH_TAIL, "");
}

export function vaultBreadcrumb(path: string): string {
  const parts = path
    .split("/")
    .filter(Boolean)
    .map((part) => humanizeStem(part.replace(/\.md$/i, "")));
  const folderParts =
    parts.length <= 1
      ? parts
      : parts.slice(0, -1).filter((part, index, all) => {
          if (index === 0) return true;
          return part.toLowerCase() !== all[index - 1]!.toLowerCase();
        });
  if (folderParts.length === 0) return "";
  if (folderParts.length === 1) return folderParts[0] ?? "";
  return folderParts.join(" › ");
}

/** Disambiguate duplicate display titles in the vault tree. */
export function buildVaultLabelMap(notes: VaultNote[]): Map<string, string> {
  const baseByPath = new Map<string, string>();
  for (const note of notes) {
    baseByPath.set(note.path, vaultDisplayTitle(note.title, note.path));
  }

  const groups = new Map<string, string[]>();
  for (const [path, base] of baseByPath) {
    const key = base.toLowerCase();
    const bucket = groups.get(key) ?? [];
    bucket.push(path);
    groups.set(key, bucket);
  }

  const result = new Map<string, string>();
  for (const note of notes) {
    const base = baseByPath.get(note.path) ?? note.path;
    const siblings = groups.get(base.toLowerCase()) ?? [note.path];
    if (siblings.length === 1) {
      result.set(note.path, base);
      continue;
    }
    const suffix = formatNoteDate(note.modified_at_utc) ?? shortPathSuffix(note.path);
    result.set(note.path, `${base} · ${suffix}`);
  }

  return result;
}

export function wikilinkLabel(target: string, titleByPath?: Map<string, string>): string {
  const normalized = target.replace(/^\[\[|\]\]$/g, "").trim();
  if (titleByPath?.has(normalized)) {
    return titleByPath.get(normalized)!;
  }
  if (titleByPath) {
    const targetStem = filenameStem(normalized);
    for (const [path, label] of titleByPath) {
      if (
        path === normalized ||
        path.endsWith(`/${normalized}`) ||
        filenameStem(path) === targetStem
      ) {
        return label;
      }
    }
  }
  return humanizeStem(targetStem(normalized));
}

function targetStem(value: string): string {
  return filenameStem(value.replace(/^\[\[|\]\]$/g, "").trim());
}

function filenameStem(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

function humanizeStem(stem: string): string {
  const withoutHash = stem.replace(HASH_TAIL, "").replace(EMBEDDED_HEX, "");
  return withoutHash
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (char) => char.toUpperCase())
    .trim();
}

function looksLikeHashSlug(value: string): boolean {
  const stripped = stripEmbeddedHashes(value);
  if (!stripped || PURE_HEX.test(stripped)) return true;
  return HASH_TAIL.test(value) || /[0-9a-f]{16,}/i.test(value);
}

function formatNoteDate(iso: string): string | null {
  try {
    return new Date(iso).toLocaleDateString([], { month: "short", day: "numeric" });
  } catch {
    return null;
  }
}

function shortPathSuffix(path: string): string {
  const stem = filenameStem(path);
  const match = stem.match(/[0-9a-f]{6,}$/i);
  return match ? `…${match[0].slice(-4)}` : stem.slice(-6);
}
