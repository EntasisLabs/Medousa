import { readExternalFile } from "$lib/utils/externalDeskApi";
import { writeAbsoluteTextFile } from "$lib/utils/vaultFilesystem";
import { activeWorkshopId, isCoLocatedWorkshop } from "$lib/utils/workshopLocality";

export type FileDocument = { path: string; original: string; draft: string; saving: boolean; error: string | null };
export class FileDocuments {
  documents = $state<Record<string, FileDocument>>({});
  private pending = new Map<string, Promise<FileDocument>>();
  private flushers = new Map<string, Set<() => void>>();
  private key(path: string) { return JSON.stringify([activeWorkshopId(), path]); }
  get(path: string) { return this.documents[this.key(path)]; }
  dirty(path: string) { const doc = this.get(path); return !!doc && doc.original !== doc.draft; }
  async open(path: string): Promise<FileDocument> {
    const key = this.key(path);
    const existing = this.documents[key];
    if (existing) return existing;
    const pending = this.pending.get(key);
    if (pending) return pending;
    const request = readExternalFile(path).then((payload) => {
      if (key !== this.key(path)) throw new Error("The workshop changed while opening this file.");
      if (payload.kind !== "text") throw new Error("This file does not contain editable UTF-8 text.");
      this.documents[key] = { path, original: payload.content, draft: payload.content, saving: false, error: null };
      return this.documents[key];
    }).finally(() => this.pending.delete(key));
    this.pending.set(key, request);
    return request;
  }
  register(path: string, flush: () => void) {
    const key = this.key(path);
    const callbacks = this.flushers.get(key) ?? new Set();
    callbacks.add(flush); this.flushers.set(key, callbacks);
    return () => { callbacks.delete(flush); if (!callbacks.size) this.flushers.delete(key); };
  }
  flushAll() { for (const callbacks of this.flushers.values()) for (const flush of callbacks) flush(); }
  hasDirty() { return Object.values(this.documents).some((doc) => doc.draft !== doc.original); }
  flush(path: string) { for (const flush of this.flushers.get(this.key(path)) ?? []) flush(); }
  async save(path: string) {
    this.flush(path);
    const key = this.key(path); const doc = this.documents[key];
    if (!doc || doc.saving || doc.draft === doc.original) return;
    doc.saving = true; doc.error = null;
    const content = doc.draft;
    try {
      const current = await readExternalFile(path);
      if (current.kind !== "text" || current.content !== doc.original) throw new Error("This file changed on disk. Your draft is kept; reopen the file after saving a copy or reconciling the changes.");
      if (key !== this.key(path) || !isCoLocatedWorkshop()) throw new Error("Return to the file’s workshop before saving.");
      await writeAbsoluteTextFile(path, content);
      doc.original = content;
    } catch (error) { doc.error = error instanceof Error ? error.message : String(error); }
    finally {
      doc.saving = false;
      if (!this.flushers.has(key) && doc.original === doc.draft) {
        const next = { ...this.documents }; delete next[key]; this.documents = next;
      }
    }
  }
  release(path: string) { if (!this.flushers.has(this.key(path)) && !this.dirty(path) && !this.get(path)?.saving) this.close(path); }
  close(path: string) { const next = { ...this.documents }; delete next[this.key(path)]; this.documents = next; }
}
export const fileDocuments = new FileDocuments();
