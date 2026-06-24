import { resolveSpaceForPath } from "$lib/config/vaultSpaces";

const RECENT_KEY = "medousa-home-vault-recent";
const MAX_RECENT = 8;
const DEFAULT_GROUP_LIMIT = 3;

export function loadVaultRecent(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed)
      ? parsed.filter((entry): entry is string => typeof entry === "string").slice(0, MAX_RECENT)
      : [];
  } catch {
    return [];
  }
}

export function rememberVaultRecent(path: string) {
  const trimmed = path.trim();
  if (!trimmed || typeof localStorage === "undefined") return;
  const next = [trimmed, ...loadVaultRecent().filter((entry) => entry !== trimmed)].slice(
    0,
    MAX_RECENT,
  );
  localStorage.setItem(RECENT_KEY, JSON.stringify(next));
}

function normalizeFolderPrefix(prefix: string): string {
  const trimmed = prefix.trim();
  if (!trimmed) return "";
  return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
}

export function recentPathsForSpace(
  recentPaths: readonly string[],
  spaceId: string,
  notes: readonly { path: string; title: string }[],
  limit = DEFAULT_GROUP_LIMIT,
  excludePath?: string | null,
): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const path of recentPaths) {
    if (path === excludePath) continue;
    const note = notes.find((entry) => entry.path === path);
    if (!note) continue;
    if (resolveSpaceForPath(path, note.title).id !== spaceId) continue;
    if (seen.has(path)) continue;
    seen.add(path);
    result.push(path);
    if (result.length >= limit) break;
  }
  return result;
}

export function recentPathsForFolder(
  recentPaths: readonly string[],
  folderPrefix: string,
  knownPaths: ReadonlySet<string>,
  limit = DEFAULT_GROUP_LIMIT,
  excludePath?: string | null,
): string[] {
  const prefix = normalizeFolderPrefix(folderPrefix);
  if (!prefix) return [];
  const result: string[] = [];
  for (const path of recentPaths) {
    if (path === excludePath) continue;
    if (!knownPaths.has(path)) continue;
    if (!path.startsWith(prefix)) continue;
    const remainder = path.slice(prefix.length);
    if (!remainder || remainder.includes("/")) continue;
    result.push(path);
    if (result.length >= limit) break;
  }
  return result;
}
