/** Keyboard shortcuts owned by the Code source editor window handler. */

export type CodeEditorWindowKeyContext = {
  interactive: boolean;
  refactorOpen: boolean;
  refactorApplying: boolean;
  clearRefactorPreview: () => void;
  renameOpen: boolean;
  quickOpen: boolean;
  editorMenuOpen: boolean;
  editable: boolean;
  canBeginEdit: boolean;
  canRename: boolean;
  hasActiveTab: boolean;
  isActiveDirty: boolean;
  problemsPanelOpen: boolean;
  canNavigate: (direction: -1 | 1) => boolean;
  showQuickOpen: () => void;
  closeQuickOpen: () => void;
  navigate: (direction: -1 | 1) => void;
  reopenClosedTab: () => void;
  saveAll: () => void;
  saveActive: () => void;
  canSaveShortcut: boolean;
  toggleTerminal: () => void;
  openSearch: () => void;
  showOutline: () => void;
  beginRename: () => void;
  clearProblemsPanel: () => void;
};

export function handleCodeEditorWindowKeydown(
  event: KeyboardEvent,
  ctx: CodeEditorWindowKeyContext,
): void {
  if (!ctx.interactive) return;
  if (ctx.refactorOpen) {
    if (event.key === "Escape" && !ctx.refactorApplying) {
      event.preventDefault();
      ctx.clearRefactorPreview();
    }
    return;
  }
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "p") {
    event.preventDefault();
    void ctx.showQuickOpen();
  }
  if (event.altKey && !event.metaKey && !event.ctrlKey) {
    if (event.key === "ArrowLeft" && ctx.canNavigate(-1)) {
      event.preventDefault();
      void ctx.navigate(-1);
    } else if (event.key === "ArrowRight" && ctx.canNavigate(1)) {
      event.preventDefault();
      void ctx.navigate(1);
    }
  }
  if (event.key === "Escape" && ctx.quickOpen) ctx.closeQuickOpen();
  if (event.defaultPrevented) return;
  if (ctx.renameOpen || ctx.quickOpen || ctx.editorMenuOpen) return;
  const command = event.metaKey || event.ctrlKey;
  if (command && event.shiftKey && event.key.toLowerCase() === "t") {
    event.preventDefault();
    void ctx.reopenClosedTab();
    return;
  }
  if (command && event.shiftKey && event.key.toLowerCase() === "s") {
    event.preventDefault();
    if (ctx.canSaveShortcut) void ctx.saveAll();
    return;
  }
  if (command && event.key.toLowerCase() === "s") {
    event.preventDefault();
    if (ctx.canSaveShortcut && ctx.hasActiveTab && ctx.isActiveDirty) {
      void ctx.saveActive();
    }
    return;
  }
  if (command && event.key === "`") {
    event.preventDefault();
    void ctx.toggleTerminal();
    return;
  }
  if (command && event.shiftKey && event.key.toLowerCase() === "f") {
    event.preventDefault();
    ctx.openSearch();
    return;
  }
  if (command && event.shiftKey && event.key.toLowerCase() === "o" && ctx.hasActiveTab) {
    event.preventDefault();
    void ctx.showOutline();
  }
  if (event.key === "F2" && ctx.canRename && ctx.editable && ctx.hasActiveTab) {
    event.preventDefault();
    ctx.beginRename();
    return;
  }
  if (event.key === "Escape" && ctx.problemsPanelOpen) ctx.clearProblemsPanel();
}
