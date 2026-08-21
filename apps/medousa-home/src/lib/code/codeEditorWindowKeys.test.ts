import { beforeEach, describe, expect, it, vi } from "vitest";

import { clearChordOverrides, setChordOverride } from "$lib/commands/commandBindings";
import { handleCodeEditorWindowKeydown } from "./codeEditorWindowKeys";

function key(
  value: string,
  options: Partial<KeyboardEvent> = {},
): KeyboardEvent {
  return {
    key: value,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    defaultPrevented: false,
    preventDefault: vi.fn(),
    ...options,
  } as unknown as KeyboardEvent;
}

function context() {
  return {
    interactive: true,
    refactorOpen: false,
    refactorApplying: false,
    clearRefactorPreview: vi.fn(),
    renameOpen: false,
    quickOpen: false,
    editorMenuOpen: false,
    editable: true,
    canBeginEdit: true,
    canRename: true,
    canFormat: true,
    hasActiveTab: true,
    isActiveDirty: true,
    problemsPanelOpen: false,
    canNavigate: vi.fn(() => true),
    showQuickOpen: vi.fn(),
    closeQuickOpen: vi.fn(),
    navigate: vi.fn(),
    reopenClosedTab: vi.fn(),
    saveAll: vi.fn(),
    saveActive: vi.fn(),
    canSaveShortcut: true,
    toggleTerminal: vi.fn(),
    openSearch: vi.fn(),
    showOutline: vi.fn(),
    beginRename: vi.fn(),
    formatDocument: vi.fn(),
    clearProblemsPanel: vi.fn(),
  };
}

describe("Code editor workbench key routing", () => {
  beforeEach(() => clearChordOverrides());

  it("does not turn the command-palette chord into Quick Open", () => {
    const ctx = context();
    handleCodeEditorWindowKeydown(
      key("p", { metaKey: true, shiftKey: true }),
      ctx,
    );
    expect(ctx.showQuickOpen).not.toHaveBeenCalled();
  });

  it("honors remapped Code commands", () => {
    const ctx = context();
    setChordOverride("editor.action.formatDocument", "mod:Alt+L");
    handleCodeEditorWindowKeydown(key("l", { metaKey: true, altKey: true }), ctx);
    expect(ctx.formatDocument).toHaveBeenCalledOnce();
  });
});
