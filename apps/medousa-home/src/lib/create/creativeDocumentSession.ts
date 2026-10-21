import type { CreativeApi } from "./creativeApi";
import type { CreativeDocument, CreativeMutationResponse, CreativeOperation, CreativeTransaction, CreativeHistoryRequest } from "$lib/types/generated/daemon_api";
import { randomUuid } from "$lib/utils/randomUuid";

/** Headless shared document session. Retain one instance per workshop/document in
 * the shell owner, then lend it to canvases/embeds. No renderer or camera is owned
 * here. A failed save retains the exact request ID and base revision for retry.
 */
export function creativeDocumentSession(api: Pick<CreativeApi, "get" | "transact" | "history">, id: string) {
  let document: CreativeDocument | null = null;
  let pending: CreativeTransaction | null = null;
  let pendingHistory: CreativeHistoryRequest | null = null;
  let busy = false;
  let canUndo = false;
  let canRedo = false;
  const listeners = new Set<() => void>();
  const notify = () => listeners.forEach(listener => listener());
  function freeze<T>(value: T): T {
    if (value && typeof value === "object" && !Object.isFrozen(value)) {
      Object.freeze(value);
      for (const child of Object.values(value)) freeze(child);
    }
    return value;
  }
  const accept = (response: CreativeMutationResponse) => {
    document = freeze(response.document); canUndo = response.can_undo; canRedo = response.can_redo;
  };
  return {
    get document() { return document; },
    get pending() { return pending ? structuredClone(pending) : null; },
    get pendingHistory() { return pendingHistory ? { ...pendingHistory } : null; },
    get busy() { return busy; },
    get dirty() { return pending !== null || pendingHistory !== null; },
    get canUndo() { return canUndo; },
    get canRedo() { return canRedo; },
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
    readRemote() { return api.get(id); },
    async load() {
      if (busy || pending || pendingHistory) throw new Error("Resolve the pending edit before refreshing this document");
      busy = true; notify();
      try { accept(await api.get(id)); } finally { busy = false; notify(); }
    },
    stage(label: string, operations: CreativeOperation[]) {
      if (busy || pending || pendingHistory || !document) throw new Error("Load the document and finish its pending edit first");
      const next: CreativeTransaction = { id: randomUuid(), expected_revision: document.revision, label, operations: structuredClone(operations) };
      if (new TextEncoder().encode(JSON.stringify(next)).length > 2 * 1024 * 1024) throw new Error("This edit is too large; split it into smaller gestures");
      pending = next; notify();
    },
    async save() {
      if (busy || !pending) throw new Error("There is no available pending edit to save");
      busy = true; notify();
      try { accept(await api.transact(id, structuredClone(pending))); pending = null; }
      finally { busy = false; notify(); }
    },
    discardPending() {
      if (busy) throw new Error("Wait for the in-flight save before discarding its draft");
      pending = null; pendingHistory = null; notify();
    },
    async history(redo = false) {
      if (busy || pending || !document) throw new Error("Finish the pending edit before changing history");
      if (pendingHistory && pendingHistory.redo !== redo) throw new Error("Resolve the pending history request first");
      pendingHistory ??= { id: randomUuid(), expected_revision: document.revision, redo };
      busy = true; notify();
      try { accept(await api.history(id, { ...pendingHistory })); pendingHistory = null; }
      finally { busy = false; notify(); }
    },
  };
}
