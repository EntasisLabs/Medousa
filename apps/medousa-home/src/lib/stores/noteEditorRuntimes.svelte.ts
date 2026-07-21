/**
 * Path-keyed note editor UI sessions (ChatSessionRuntime analogue).
 * Markdown/dirty/hash stay in vault noteBuffers; this owns plane/find/scroll.
 */

import {
  cloneNoteEditorRuntime,
  emptyNoteEditorRuntime,
  type NoteEditorRuntime,
  type NoteEditorUiState,
} from "$lib/stores/noteEditorRuntime";

const MAX_RUNTIMES = 48;

export class NoteEditorRuntimeStore {
  /** Bumped when map membership or lastFocusedAt changes (LRU / keep-alive). */
  revision = $state(0);
  private runtimes = new Map<string, NoteEditorRuntime>();

  private bump() {
    this.revision += 1;
  }

  private normalize(path: string): string {
    return path.trim();
  }

  get(path: string): NoteEditorRuntime | null {
    void this.revision;
    const key = this.normalize(path);
    if (!key) return null;
    const runtime = this.runtimes.get(key);
    return runtime ? cloneNoteEditorRuntime(runtime) : null;
  }

  ensure(path: string, defaults?: Partial<NoteEditorUiState>): NoteEditorRuntime {
    const key = this.normalize(path);
    let runtime = this.runtimes.get(key);
    if (!runtime) {
      runtime = emptyNoteEditorRuntime(key, defaults);
      this.runtimes.set(key, runtime);
      this.trim();
      this.bump();
    }
    return cloneNoteEditorRuntime(runtime);
  }

  touch(path: string) {
    const key = this.normalize(path);
    if (!key) return;
    const runtime = this.runtimes.get(key) ?? emptyNoteEditorRuntime(key);
    runtime.lastFocusedAt = Date.now();
    this.runtimes.set(key, runtime);
    this.bump();
  }

  patchUi(path: string, patch: Partial<NoteEditorUiState>) {
    const key = this.normalize(path);
    if (!key) return;
    const runtime = this.runtimes.get(key) ?? emptyNoteEditorRuntime(key);
    runtime.ui = { ...runtime.ui, ...patch };
    if (patch.selection) runtime.ui.selection = { ...patch.selection };
    if (patch.find) runtime.ui.find = { ...patch.find };
    this.runtimes.set(key, runtime);
    this.bump();
  }

  lastFocusedAt(path: string): number {
    void this.revision;
    return this.runtimes.get(this.normalize(path))?.lastFocusedAt ?? 0;
  }

  drop(path: string) {
    const key = this.normalize(path);
    if (!this.runtimes.delete(key)) return;
    this.bump();
  }

  /** Paths ranked newest-focus first (for TipTap keep-alive LRU). */
  rankedPaths(): string[] {
    void this.revision;
    return [...this.runtimes.values()]
      .sort((a, b) => b.lastFocusedAt - a.lastFocusedAt)
      .map((runtime) => runtime.path);
  }

  private trim() {
    if (this.runtimes.size <= MAX_RUNTIMES) return;
    const ranked = [...this.runtimes.values()].sort(
      (a, b) => a.lastFocusedAt - b.lastFocusedAt,
    );
    while (this.runtimes.size > MAX_RUNTIMES && ranked.length) {
      const oldest = ranked.shift();
      if (oldest) this.runtimes.delete(oldest.path);
    }
  }
}

export const noteEditorRuntimes = new NoteEditorRuntimeStore();
