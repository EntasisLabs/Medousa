/** Human-facing vault labels — hide hash tails in primary UI. */

const HASH_TAIL = /-[0-9a-f]{8,}(?=\.md$)/i;

export function vaultDisplayTitle(title: string, path?: string | null): string {
  const trimmed = title.trim();
  if (trimmed && !looksLikeHashSlug(trimmed)) {
    return trimmed;
  }
  const fromPath = path ? filenameStem(path) : trimmed;
  return humanizeStem(fromPath);
}

export function vaultDisplayPath(path: string): string {
  return path.replace(HASH_TAIL, "");
}

export function wikilinkLabel(target: string, titleByPath?: Map<string, string>): string {
  const normalized = target.replace(/^\[\[|\]\]$/g, "").trim();
  const titled = titleByPath?.get(normalized);
  if (titled) return vaultDisplayTitle(titled, normalized);
  return humanizeStem(filenameStem(normalized));
}

function filenameStem(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

function humanizeStem(stem: string): string {
  const withoutHash = stem.replace(HASH_TAIL, "");
  return withoutHash
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (char) => char.toUpperCase())
    .trim();
}

function looksLikeHashSlug(value: string): boolean {
  return HASH_TAIL.test(value) || /^[0-9a-f-]{20,}$/i.test(value);
}
