/** Pure landing and Files-filter rules for the mobile Code workspace. */

export type MobileCodeSurface = "files" | "editor" | "terminal" | "changes";

export type MobileCodeJumpOrigin = "files" | "changes" | "terminal";

export type MobileCodeFilesFilter = "changed" | "recent" | "tree";

export type MobileCodeChromeMode =
  | "projects"
  | "files"
  | "editor"
  | "terminal"
  | "changes";

export function projectHasAttention(input: {
  humanPhase?: string | null;
  forgeState?: string | null;
  dirtyWorkingCopy?: boolean;
  dirtyBuffers?: boolean;
}): boolean {
  const phase = input.humanPhase ?? "";
  const state = input.forgeState ?? "";
  if (phase === "review" || phase === "needs_attention") return true;
  if (state === "awaiting_review" || state === "applying_decision") return true;
  return Boolean(input.dirtyWorkingCopy || input.dirtyBuffers);
}

export function resolveMobileCodeLanding(input: {
  hasAttention: boolean;
  hasOpenFile: boolean;
}): MobileCodeSurface {
  if (input.hasAttention) return "changes";
  if (input.hasOpenFile) return "editor";
  return "files";
}

export function resolveMobileCodeFilesFilter(input: {
  hasChangedFiles: boolean;
  hasRecentFiles: boolean;
}): MobileCodeFilesFilter {
  if (input.hasChangedFiles) return "changed";
  if (input.hasRecentFiles) return "recent";
  return "tree";
}
