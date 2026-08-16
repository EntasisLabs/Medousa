/** Per-path note editor UI session (ChatSessionRuntime analogue). */

export type NoteEditorUiState = {
  plane: "live" | "build";
  editorMode: "edit" | "preview";
  editorSurface: "write" | "source";
  scrollTop: number;
  selection?: { from: number; to: number };
  find?: { query: string; matchIndex: number; matchCase: boolean };
};

export type NoteEditorRuntime = {
  path: string;
  ui: NoteEditorUiState;
  /** Last time this runtime was the focused editable host (LRU for TipTap pool). */
  lastFocusedAt: number;
};

export function defaultNoteEditorUi(defaults?: Partial<NoteEditorUiState>): NoteEditorUiState {
  return {
    plane: defaults?.plane ?? "live",
    editorMode: defaults?.editorMode ?? "edit",
    editorSurface: defaults?.editorSurface ?? "write",
    scrollTop: defaults?.scrollTop ?? 0,
    selection: defaults?.selection,
    find: defaults?.find,
  };
}

export function emptyNoteEditorRuntime(
  path: string,
  defaults?: Partial<NoteEditorUiState>,
): NoteEditorRuntime {
  return {
    path,
    ui: defaultNoteEditorUi(defaults),
    lastFocusedAt: Date.now(),
  };
}

export function cloneNoteEditorRuntime(runtime: NoteEditorRuntime): NoteEditorRuntime {
  return {
    path: runtime.path,
    lastFocusedAt: runtime.lastFocusedAt,
    ui: {
      ...runtime.ui,
      selection: runtime.ui.selection
        ? { ...runtime.ui.selection }
        : undefined,
      find: runtime.ui.find ? { ...runtime.ui.find } : undefined,
    },
  };
}
