/**
 * Shared forge source-tree fetches.
 * Cold open used to fire land + explorer /tree in parallel with no sharing;
 * this coalesces in-flight requests and keeps a short TTL so the second caller
 * reuses the first response instead of stampeding the daemon.
 */

import {
  getUndertakingSourceTree,
  type ForgeSourceTree,
} from "$lib/forge";

const inflight = new Map<string, Promise<ForgeSourceTree>>();
const cache = new Map<string, { tree: ForgeSourceTree; at: number }>();

/** Short enough that edits still refresh; long enough to cover land+explorer open. */
const TTL_MS = 4_000;

export function peekUndertakingSourceTree(workId: string): ForgeSourceTree | null {
  const id = workId.trim();
  if (!id) return null;
  const hit = cache.get(id);
  if (!hit) return null;
  if (Date.now() - hit.at > TTL_MS) {
    cache.delete(id);
    return null;
  }
  return hit.tree;
}

export function invalidateUndertakingSourceTree(workId?: string) {
  if (!workId?.trim()) {
    cache.clear();
    inflight.clear();
    return;
  }
  const id = workId.trim();
  cache.delete(id);
  inflight.delete(id);
}

export function getUndertakingSourceTreeShared(
  workId: string,
  options?: { force?: boolean },
): Promise<ForgeSourceTree> {
  const id = workId.trim();
  if (!id) {
    return Promise.reject(new Error("No project selected."));
  }

  if (!options?.force) {
    const cached = peekUndertakingSourceTree(id);
    if (cached) return Promise.resolve(cached);
    const pending = inflight.get(id);
    if (pending) return pending;
  }

  const request = getUndertakingSourceTree(id)
    .then((tree) => {
      cache.set(id, { tree, at: Date.now() });
      return tree;
    })
    .finally(() => {
      if (inflight.get(id) === request) inflight.delete(id);
    });

  inflight.set(id, request);
  return request;
}
