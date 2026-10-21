import type { CreativeApi } from "./creativeApi";
import { creativeDocumentSession } from "./creativeDocumentSession";

type Session = ReturnType<typeof creativeDocumentSession>;
type Entry = { session: Session; readers: number };
/** Shell-owned sharing for tabs, splits and embeds. The capacity bounds retained
 * scenes; active or dirty sessions are never evicted to make room for another.
 * Bitmap bytes and decoded images do not belong in this pool.
 */
export function creativeSessionPool(capacity = 8) {
  if (!Number.isInteger(capacity) || capacity < 1 || capacity > 32) throw new Error("Invalid creative session capacity");
  const entries = new Map<string, Entry>();
  return {
    get size() { return entries.size; },
    acquire(api: CreativeApi, documentId: string) {
      const key = JSON.stringify([api.scope, documentId]);
      let entry = entries.get(key);
      if (!entry) {
        if (entries.size >= capacity) {
          const victim = [...entries].find(([, candidate]) => candidate.readers === 0 && !candidate.session.busy && !candidate.session.dirty);
          if (!victim) throw new Error("Finish or discard an existing creative draft before opening another project");
          entries.delete(victim[0]);
        }
        entry = { session: creativeDocumentSession(api, documentId), readers: 0 };
      }
      entry.readers += 1;
      entries.delete(key); entries.set(key, entry);
      const retained = entry;
      let released = false;
      return {
        session: entry.session,
        release() { if (!released) { released = true; retained.readers -= 1; } },
      };
    },
  };
}
