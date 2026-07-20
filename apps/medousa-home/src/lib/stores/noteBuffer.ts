/** Path-keyed note snapshot for multi-pane Workspace (chat sessionRuntimes analogue). */

export type NoteBuffer = {
  path: string;
  content: string;
  baselineContent: string;
  contentHash: string | null;
  title: string;
  dirty: boolean;
  contentRevision: number;
};

export function emptyNoteBuffer(path: string): NoteBuffer {
  return {
    path,
    content: "",
    baselineContent: "",
    contentHash: null,
    title: "",
    dirty: false,
    contentRevision: 0,
  };
}

export function cloneNoteBuffer(buffer: NoteBuffer): NoteBuffer {
  return { ...buffer };
}
