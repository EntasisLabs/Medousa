/** Split `path:revision` without breaking absolute OS paths (`/tmp/a.md:3`, `C:/a.md:3`). */
export function splitContentSyncKey(key: string): { path: string; revision: string } {
  const idx = key.lastIndexOf(":");
  if (idx <= 0) return { path: key, revision: "0" };
  const revision = key.slice(idx + 1);
  if (!/^\d+$/.test(revision)) return { path: key, revision: "0" };
  return { path: key.slice(0, idx), revision };
}
