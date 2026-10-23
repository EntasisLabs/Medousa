import type { CreativeApi } from "./creativeApi";
import type {
  CreativeDocument,
  CreativeMutationResponse,
  CreativeOperation,
  CreativeTransaction,
  CreativeHistoryRequest,
  CreativeScene,
} from "$lib/types/generated/daemon_api";
import { applySceneOperations } from "./creativeScene";
import { randomUuid } from "$lib/utils/randomUuid";

/** Headless shared document session. Retain one instance per workshop/document in
 * the shell owner, then lend it to canvases/embeds. No renderer or camera is owned
 * here. A failed save retains the exact request ID and base revision for retry.
 */
export function creativeDocumentSession(
  api: Pick<CreativeApi, "get" | "transact" | "history">,
  id: string,
) {
  let document: CreativeDocument | null = null;
  let pending: CreativeTransaction | null = null;
  let pendingHistory: CreativeHistoryRequest | null = null;
  let busy = false;
  let action: "load" | "save" | "history" | "delete" | null = null;
  let deleted = false;
  let failed = false;
  let queue: { id: string; label: string; operations: CreativeOperation[] }[] =
    [];
  let saving: Promise<void> | null = null;
  const bytes = (value: unknown) =>
    new TextEncoder().encode(JSON.stringify(value)).length;
  function promote(expectedRevision = document?.revision) {
    if (!pending && queue.length && expectedRevision !== undefined)
      pending = { ...queue.shift()!, expected_revision: expectedRevision };
  }
  let canUndo = false;
  let canRedo = false;
  let scene: CreativeScene | null = null;
  const listeners = new Set<() => void>();
  const notify = () => {
    if (document) {
      const next = applySceneOperations(document.scene, [
        ...(pending?.operations ?? []),
        ...queue.flatMap((edit) => edit.operations),
      ]);
      // A save receipt acknowledges pixels already on screen. Preserve their
      // identity, including unchanged nodes, instead of rebuilding the canvas.
      const previous = new Map(scene?.nodes.map((node) => [node.id, node]));
      next.nodes = next.nodes.map((node) => {
        const old = previous.get(node.id);
        return old && (old === node || JSON.stringify(old) === JSON.stringify(node))
          ? old : node;
      });
      if (!scene || scene.background !== next.background ||
          scene.nodes.length !== next.nodes.length ||
          next.nodes.some((node, i) => node !== scene!.nodes[i])) scene = freeze(next);
    }
    listeners.forEach((listener) => listener());
  };
  function freeze<T>(value: T): T {
    if (value && typeof value === "object" && !Object.isFrozen(value)) {
      Object.freeze(value);
      for (const child of Object.values(value)) freeze(child);
    }
    return value;
  }
  const accept = (response: CreativeMutationResponse) => {
    document = freeze(response.document);
    canUndo = response.can_undo;
    canRedo = response.can_redo;
  };
  return {
    get document() {
      return document;
    },
    get pending() {
      return pending ? structuredClone(pending) : null;
    },
    get pendingHistory() {
      return pendingHistory ? { ...pendingHistory } : null;
    },
    get busy() {
      return busy;
    },
    get dirty() {
      return pending !== null || pendingHistory !== null || queue.length > 0;
    },
    get canEdit() {
      return (
        document !== null && !deleted &&
        !failed &&
        (action === null || action === "save") &&
        !pendingHistory
      );
    },
    get failed() {
      return failed;
    },
    get scene() {
      return scene;
    },
    get canUndo() {
      return canUndo;
    },
    get canRedo() {
      return canRedo;
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    readRemote() {
      return api.get(id);
    },
    /** Refresh a clean scene without locking gestures or replacing an edit made
     * while the request was in flight. Concurrent drafts retain revision guards. */
    async refreshIfClean() {
      if (!document || deleted || busy || pending || pendingHistory || queue.length) return;
      const base = document;
      const response = await api.get(id);
      if (document !== base || deleted || busy || pending || pendingHistory || queue.length) return;
      if (response.document.revision > base.revision) { accept(response); notify(); }
    },
    async load() {
      if (deleted) throw new Error("This project has been deleted");
      if (busy || pending || pendingHistory || queue.length)
        throw new Error(
          "Resolve the pending edit before refreshing this document",
        );
      busy = true;
      action = "load";
      notify();
      try {
        accept(await api.get(id));
        failed = false;
      } finally {
        busy = false;
        action = null;
        notify();
      }
    },
    stage(label: string, operations: CreativeOperation[]) {
      if (busy || pending || pendingHistory || !document)
        throw new Error("Load the document and finish its pending edit first");
      const next: CreativeTransaction = {
        id: randomUuid(),
        expected_revision: document.revision,
        label,
        operations: structuredClone(operations),
      };
      if (
        new TextEncoder().encode(JSON.stringify(next)).length >
        2 * 1024 * 1024
      )
        throw new Error(
          "This edit is too large; split it into smaller gestures",
        );
      pending = next;
      notify();
    },
    enqueue(label: string, operations: CreativeOperation[]) {
      if (!document || failed || pendingHistory || (busy && action !== "save"))
        throw new Error("Resolve the pending edit before continuing");
      const edit = {
        id: randomUuid(),
        label,
        operations: structuredClone(operations),
      };
      if (
        bytes({ ...edit, expected_revision: document.revision }) >
        2 * 1024 * 1024
      )
        throw new Error(
          "This edit is too large; split it into smaller gestures",
        );
      if (
        queue.length >= 32 ||
        bytes([pending, ...queue, edit]) > 8 * 1024 * 1024
      )
        throw new Error(
          "Wait for the workshop to save your current edits before drawing more",
        );
      queue.push(edit);
      promote();
      notify();
    },
    async remove(remove: CreativeApi["delete"]) {
      if (busy || pending || pendingHistory || queue.length || !document || deleted)
        throw new Error("Finish saving this project before deleting it");
      busy = true;
      action = "delete";
      notify();
      try {
        await remove(id, document.revision);
        deleted = true;
        document = null;
        scene = null;
        canUndo = false;
        canRedo = false;
      } finally {
        busy = false;
        action = null;
        notify();
      }
    },
    save(): Promise<void> {
      if (saving) return saving;
      promote();
      if (busy || !pending)
        return Promise.reject(
          new Error("There is no available pending edit to save"),
        );
      busy = true;
      action = "save";
      failed = false;
      notify();
      // Start after saving is assigned, so concurrent callers share one drain.
      saving = Promise.resolve().then(async () => {
        try {
          while (pending) {
            const response = await api.transact(id, structuredClone(pending));
            accept(response);
            pending = null;
            // An idempotent retry can return a newer remote document. Keep the
            // tail pinned to our acknowledged revision rather than rebasing it
            // silently over another client's changes.
            promote(response.applied_revision);
            notify();
          }
        } catch (error) {
          failed = true;
          throw error;
        } finally {
          busy = false;
          action = null;
          saving = null;
          notify();
        }
      });
      return saving;
    },
    discardPending() {
      if (busy)
        throw new Error(
          "Wait for the in-flight save before discarding its draft",
        );
      pending = null;
      pendingHistory = null;
      queue = [];
      failed = false;
      notify();
    },
    async history(redo = false) {
      if (busy || pending || queue.length || !document)
        throw new Error("Finish the pending edit before changing history");
      if (pendingHistory && pendingHistory.redo !== redo)
        throw new Error("Resolve the pending history request first");
      pendingHistory ??= {
        id: randomUuid(),
        expected_revision: document.revision,
        redo,
      };
      busy = true;
      action = "history";
      notify();
      try {
        accept(await api.history(id, { ...pendingHistory }));
        pendingHistory = null;
        failed = false;
      } finally {
        busy = false;
        action = null;
        notify();
      }
    },
  };
}
