/**
 * Pure decision helpers for Code editor save / lease gating.
 * Keeps soft-lease (type → begin attempt) and save paths testable without the
 * CodeSourceEditor component graph.
 */

export type CodeSaveGateInput = {
  /** File is open as a non-editable preview. */
  preview: boolean;
  /** Buffer has unsaved edits. */
  dirty: boolean;
  /** A save RPC is already in flight for this editor. */
  savingFile: boolean;
  /** Human lease is present (workId matches + leaseId + generation). */
  hasLease: boolean;
  /** Soft-lease: begin_attempt is allowed (no agent control, not preview). */
  canBeginEdit: boolean;
  /** A begin-attempt request is already running. */
  beginningEdit: boolean;
};

export type CodeSaveGateDecision =
  | { action: "noop"; reason: "not-dirty" | "already-saving" }
  | { action: "reject"; reason: "preview" | "no-lease" }
  | { action: "await-lease" }
  | { action: "begin-then-save" }
  | { action: "save" };

/**
 * Decide what Cmd+S / Save should do given current lease and edit state.
 * Does not perform I/O — callers await beginEditPromise / ensureHumanLease /
 * saveUndertakingSource based on the returned action.
 */
export function decideCodeSave(input: CodeSaveGateInput): CodeSaveGateDecision {
  if (input.preview) {
    return { action: "reject", reason: "preview" };
  }
  if (!input.dirty) {
    return { action: "noop", reason: "not-dirty" };
  }
  if (input.savingFile) {
    return { action: "noop", reason: "already-saving" };
  }
  if (input.hasLease) {
    return { action: "save" };
  }
  if (input.beginningEdit) {
    return { action: "await-lease" };
  }
  if (input.canBeginEdit) {
    return { action: "begin-then-save" };
  }
  return { action: "reject", reason: "no-lease" };
}

/** Whether the Save keyboard shortcut should attempt a save at all. */
export function canInvokeCodeSaveShortcut(input: {
  editable: boolean;
  canBeginEdit: boolean;
}): boolean {
  return input.editable || input.canBeginEdit;
}

export const CODE_SAVE_PREVIEW_ERROR =
  "This file is open as a preview and cannot be saved from Code.";

export const CODE_SAVE_NO_LEASE_ERROR =
  "Couldn’t save — the project isn’t ready for edits yet.";
