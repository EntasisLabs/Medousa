import { expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ saveUndertakingSource: vi.fn(), startHumanEditingSession: vi.fn() }));
vi.mock("$lib/code/codeDocumentService", () => ({
  ...api, canStartHumanEditing: () => true, humanizeForgeMessage: (message: string) => message,
}));
import { CodeSaveController, type CodeSaveControllerDeps } from "./codeSaveController.svelte";

function setup(overrides: Partial<CodeSaveControllerDeps> = {}) {
  let scope = "workshop-a/project";
  const tab = { tabId: "same-tab", path: "app.ts", draft: "unsaved", digest: "old-digest" };
  const deps: CodeSaveControllerDeps = {
    getScopeKey: () => scope, getWorkId: () => "same-id",
    getContext: () => ({ workId: "same-id", leaseId: "lease", leaseGeneration: 1 }),
    getDetail: () => ({ id: "same-id", allowed_actions: {} as never }),
    getActiveTab: () => tab, getActiveTabId: () => tab.tabId, getTabs: () => [tab],
    getEditor: () => undefined, getEditable: () => true, getCanBeginEdit: () => false,
    ensureLease: vi.fn(), onError: vi.fn(), captureEditorContext: vi.fn(), preferredAgent: () => "codex",
    updateDraft: vi.fn(), isDirty: () => true, acceptSaved: vi.fn(), setTabError: vi.fn(),
    setActiveFromItem: vi.fn(), refreshDetail: vi.fn(), ...overrides,
  };
  const save = new CodeSaveController(deps);
  return { save, deps, tab, switchWorkshop: () => { scope = "workshop-b/project"; save.resetForScope(); } };
}

it("does not stamp a new workshop's draft as saved when an old save finishes", async () => {
  let resolve!: (value: unknown) => void;
  api.saveUndertakingSource.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const { save, deps, tab, switchWorkshop } = setup();
  const pending = save.saveTab(tab);
  switchWorkshop();
  resolve({ path: "app.ts", content: "old saved content", digest: "new-digest" });
  expect(await pending).toBe(false);
  expect(deps.acceptSaved).not.toHaveBeenCalled();
  expect(tab.draft).toBe("unsaved");
  expect(save.savingFile).toBe(false);
  expect(save.saveWhisper).toBeNull();
});

it("does not write after switching while format-on-save is pending", async () => {
  api.saveUndertakingSource.mockClear();
  let resolve!: (value: boolean) => void;
  const { save, tab, switchWorkshop } = setup({ beforeSave: () => new Promise((done) => { resolve = done; }) });
  const pending = save.saveTab(tab);
  switchWorkshop();
  resolve(true);
  expect(await pending).toBe(false);
  expect(api.saveUndertakingSource).not.toHaveBeenCalled();
});

it("does not reactivate an old project when editing control arrives after switching", async () => {
  let resolve!: (value: unknown) => void;
  api.startHumanEditingSession.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const { save, deps, switchWorkshop } = setup();
  const pending = save.startEditing();
  switchWorkshop();
  resolve({ item: { id: "same-id" }, lease: { lease_id: "old", generation: 1 } });
  await pending;
  expect(deps.setActiveFromItem).not.toHaveBeenCalled();
  expect(deps.refreshDetail).not.toHaveBeenCalled();
  expect(save.beginningEdit).toBe(false);
});

it("preserves the precise save failure and draft instead of replacing it with a generic message", async () => {
  const message = "attached checkout switched branches: expected old, found new";
  api.saveUndertakingSource.mockRejectedValueOnce(new Error(message));
  const { save, deps, tab } = setup();
  await save.save();
  expect(save.saveWhisper).toBe("Save failed");
  expect(deps.onError).toHaveBeenLastCalledWith(message);
  expect(deps.setTabError).toHaveBeenLastCalledWith(tab.tabId, message);
  expect(deps.acceptSaved).not.toHaveBeenCalled();
  expect(tab.draft).toBe("unsaved");
});

it("marks a save rejected before writing as blocked", async () => {
  api.saveUndertakingSource.mockClear();
  const { save, tab } = setup({ getContext: () => null, getCanBeginEdit: () => false });
  expect(await save.saveTab(tab)).toBe(false);
  expect(save.saveWhisper).toBe("Save blocked");
  expect(api.saveUndertakingSource).not.toHaveBeenCalled();
});
