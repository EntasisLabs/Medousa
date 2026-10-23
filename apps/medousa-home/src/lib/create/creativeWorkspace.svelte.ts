import type { DrawCamera } from "$lib/draw/drawCamera";
import { creativeApi } from "./creativeApi";
import { creativeSessionPool } from "./creativeSessionPool";
import { onCreativeWindowChange } from "./creativeWindowSync";
import { creativeJobs } from "./creativeJobs";

const pool = creativeSessionPool(8);
type Session = ReturnType<
  typeof import("./creativeDocumentSession").creativeDocumentSession
>;
class Entry {
  selected = $state<string[]>([]);
  referenceViewReady = false;
  view: {
    camera: DrawCamera;
    viewport: { width: number; height: number };
  } | null = null;
  draft = $state("");
  preview = $state(false);
  jobs: ReturnType<typeof creativeJobs>;
  constructor(
    public session: Session,
    api: ReturnType<typeof creativeApi>,
    id: string,
  ) {
    this.jobs = creativeJobs(api, id, () => this.session.load());
  }
}
const entries = new Map<string, Entry>();
/** Documents and Ask drafts survive tab remounts; bitmaps remain view-owned. */
export function acquireCreativeProject(scope: string, id: string) {
  const key = JSON.stringify([scope, id]);
  let entry = entries.get(key);
  if (!entry && entries.size >= 8) {
    const victim = [...entries].find(
      ([k, e]) =>
        !live.has(k) &&
        !e.session.dirty &&
        !e.session.busy &&
        !e.draft.trim() &&
        !e.jobs.pending &&
        !e.jobs.busy &&
        !e.jobs.pendingDecision,
    );
    if (!victim)
      throw new Error(
        "Finish or clear an existing creative draft before opening another project",
      );
    entries.delete(victim[0]);
  }
  const api = creativeApi(scope),
    lease = pool.acquire(api, id);
  if (!entry) {
    const created = new Entry(lease.session, api, id);
    entry = created;
    entries.set(key, created);
  }
  if (entry.session !== lease.session) {
    entry.session = lease.session;
    entry.jobs = creativeJobs(api, id, () => lease.session.load());
  }
  live.set(key, (live.get(key) ?? 0) + 1);
  const stop = entry.jobs.retain();
  const refresh = () => { void lease.session.refreshIfClean().catch(() => undefined); };
  const stopChanges = onCreativeWindowChange(change => { if (change.scope === scope && change.id === id) refresh(); });
  // Focus is also a catch-up point after suspended/offline native windows.
  if (typeof window !== "undefined") window.addEventListener("focus", refresh);
  let released = false;
  return {
    api,
    session: lease.session,
    state: entry,
    release() {
      if (released) return;
      released = true;
      stop();
      stopChanges();
      if (typeof window !== "undefined") window.removeEventListener("focus", refresh);
      lease.release();
      const n = (live.get(key) ?? 1) - 1;
      if (n) live.set(key, n);
      else live.delete(key);
    },
  };
}
const live = new Map<string, number>();
export type CreativeProjectLease = ReturnType<typeof acquireCreativeProject>;
export function pendingScene(lease: CreativeProjectLease) {
  const document = lease.session.document;
  return document
    ? lease.session.scene
    : null;
}

/** Close only after the current autosave receipt; failures leave the view alive. */
export async function flushCreativeProject(scope: string, id: string) {
  const session = entries.get(JSON.stringify([scope, id]))?.session;
  if (!session) return;
  if (session.busy && !session.pending) {
    await new Promise<void>(resolve => {
      const stop = session.subscribe(() => { if (!session.busy) { stop(); resolve(); } });
    });
  }
  if (session.pendingHistory) { await session.history(session.pendingHistory.redo); return; }
  if (session.dirty || session.busy) await session.save();
}
