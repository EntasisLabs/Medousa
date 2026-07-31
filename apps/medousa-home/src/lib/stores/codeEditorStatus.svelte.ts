export type CodeEditorStatusSnapshot = {
  workId: string;
  path: string;
  line: number;
  totalLines: number;
  column: number;
  language: string;
  indentation: string;
  issueCount: number;
  dirty: boolean;
  saving: boolean;
  saveWhisper: string | null;
  control: string;
  languageState: "ready" | "connecting" | "editing-only";
};

class CodeEditorStatusStore {
  ownerId = $state<string | null>(null);
  snapshot = $state<CodeEditorStatusSnapshot | null>(null);

  publish(ownerId: string, snapshot: CodeEditorStatusSnapshot) {
    this.ownerId = ownerId;
    this.snapshot = snapshot;
  }

  clear(ownerId: string) {
    if (this.ownerId !== ownerId) return;
    this.ownerId = null;
    this.snapshot = null;
  }
}

export const codeEditorStatus = new CodeEditorStatusStore();
