/** Nested Live Write drafts (inputs/textareas) that commit on a debounce. */

const flushers = new Set<() => void>();

export function registerLiveDraftFlush(flush: () => void): () => void {
  flushers.add(flush);
  return () => {
    flushers.delete(flush);
  };
}

/** Promote all pending Live Write drafts into TipTap attrs before save / plane switch. */
export function flushLiveDrafts(): void {
  for (const flush of [...flushers]) {
    try {
      flush();
    } catch {
      // Surface teardown mid-flush — ignore.
    }
  }
}
