/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ active: "personal", scope: "a", roots: vi.fn(), select: vi.fn(), list: vi.fn(), changes: vi.fn(), read: vi.fn(), write: vi.fn(), backlinks: vi.fn(), detach: vi.fn() }));
vi.mock("$lib/utils/workshopLocality", async original => ({ ...await original<typeof import("$lib/utils/workshopLocality")>(), activeWorkshopId: () => api.scope, workshopScopedStorageKey: (key: string) => `${key}:${api.scope}` }));
vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({ lmeWorkspace: { detachVaultNotes: api.detach } }));
vi.mock("$lib/daemon", () => ({
  addVaultRoot: vi.fn(), createVaultNote: vi.fn(), deleteVaultNote: vi.fn(),
  listVaultRoots: api.roots, setActiveVaultRoot: api.select,
  listVaultNotes: api.list, listVaultChanges: api.changes, listVaultTags: async () => ({ tags: [] }),
  getVaultNote: api.read, getVaultBacklinks: api.backlinks, saveVaultNote: api.write,
  searchVaultNotes: async () => ({ hits: [] }),
}));
import { VaultStore } from "$lib/stores/vault.svelte";
const roots = () => ({ activeRootId: api.active, roots: ["personal", "obsidian"].map(id => ({ id, label: id, path: `/${id}`, active: id === api.active, isDefault: id === "personal", isObsidian: id === "obsidian" })) });
const note = (path: string, content: string) => ({ content, note: { path, title: content, kind: "note", content_hash: content, tags: [], wikilinks_out: [], backlinks: [] } });
let store: VaultStore;
beforeEach(() => {
  vi.clearAllMocks(); localStorage.clear(); api.active = "personal"; api.scope = "a";
  api.roots.mockImplementation(async () => roots());
  api.select.mockImplementation(async id => { api.active = id; return roots(); });
  api.list.mockImplementation(async () => ({ notes: [{ path: "same.md", title: api.active, modified_at_utc: "now" }], vault_generation: api.active === "personal" ? 90 : 2 }));
  api.changes.mockResolvedValue({ vault_generation: 90, changes: [], reset_required: false });
  api.backlinks.mockImplementation(async () => ({ backlinks: [api.active] }));
  api.read.mockImplementation(async path => note(path, api.active));
  api.write.mockImplementation(async (path, content) => note(path, content));
  store = new VaultStore();
});
afterEach(() => { store.clearAutosaveTimer(); });
it("saves to the original vault before switching, then refetches duplicate paths and resets the delta cursor", async () => {
  await store.refreshNotes(); await store.openNote("same.md");
  store.markDirty("edited personal note");
  api.write.mockImplementation(async (path, content) => { expect(api.active).toBe("personal"); return note(path, content); });
  api.changes.mockClear();
  await store.switchVaultRoot("obsidian");
  expect(api.write).toHaveBeenCalledWith("same.md", "edited personal note", expect.anything());
  expect(api.changes).not.toHaveBeenCalled();
  expect(store.notes[0].title).toBe("obsidian"); expect(store.vaultGeneration).toBe(2);
  expect(store.getBuffer("same.md")).toBeUndefined(); expect(api.detach).toHaveBeenCalledOnce();
  await store.openNote("same.md"); expect(store.content).toBe("obsidian");
});
it("drops a late listing from the previous vault", async () => {
  await store.refreshVaultRoots();
  let finish!: (value: unknown) => void;
  api.list.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const old = store.refreshNotes(); await vi.waitFor(() => expect(finish).toBeDefined());
  await store.switchVaultRoot("obsidian");
  finish({ notes: [{ path: "wrong.md", title: "personal" }], vault_generation: 90 }); await old;
  expect(store.notes.map(note => note.title)).toEqual(["obsidian"]); expect(store.vaultGeneration).toBe(2);
});
it("preserves the draft and selection when a save or root switch fails", async () => {
  await store.openNote("same.md"); store.markDirty("pending");
  api.write.mockRejectedValueOnce(new Error("Disk unavailable"));
  await expect(store.switchVaultRoot("obsidian")).rejects.toThrow("Disk unavailable");
  expect(api.select).not.toHaveBeenCalled(); expect(store.content).toBe("pending"); expect(store.dirty).toBe(true);
  store.dirty = false;
  api.select.mockRejectedValueOnce(new Error("Root unavailable"));
  await expect(store.switchVaultRoot("obsidian")).rejects.toThrow("Root unavailable");
  expect(store.selectedPath).toBe("same.md"); expect(store.content).toBe("pending"); expect(store.activeVaultRootId).toBe("personal");
});
it("loads root context before listing and keeps recent/last-note state isolated", async () => {
  await store.refreshNotes();
  expect(api.roots.mock.invocationCallOrder[0]).toBeLessThan(api.list.mock.invocationCallOrder[0]);
  await store.openNote("personal.md");
  await store.switchVaultRoot("obsidian"); await store.openNote("obsidian.md");
  await store.switchVaultRoot("personal");
  expect(store.lastNotePath).toBe("personal.md"); expect(store.recentPaths).toEqual(["personal.md"]);
});
it("does not adopt a late root response after switching workshops", async () => {
  await store.refreshVaultRoots();
  let finish!: (value: ReturnType<typeof roots>) => void;
  api.select.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const pending = store.switchVaultRoot("obsidian"); await vi.waitFor(() => expect(finish).toBeDefined());
  api.scope = "b"; store.resetForWorkshopSwitch();
  finish({ ...roots(), activeRootId: "obsidian" }); await pending;
  expect(store.activeVaultRootId).toBeNull(); expect(store.notes).toEqual([]);
});
it("ignores late save receipts after another workshop opens the same relative path", async () => {
  await store.openNote("same.md"); store.markDirty("old workshop draft");
  let finish!: (value: ReturnType<typeof note>) => void;
  api.write.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const pending = store.save(); await vi.waitFor(() => expect(finish).toBeDefined());
  api.scope = "b"; store.resetForWorkshopSwitch();
  await store.openNote("same.md"); store.markDirty("new workshop draft");
  finish(note("same.md", "old receipt")); await pending;
  expect(store.content).toBe("new workshop draft"); expect(store.dirty).toBe(true);
  expect(store.saveStatus).not.toBe("saved");
});
it("does not let an old reload replace a note from the next vault", async () => {
  await store.openNote("same.md");
  let finish!: (value: ReturnType<typeof note>) => void;
  api.read.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const pending = store.reloadFromServer(); await vi.waitFor(() => expect(finish).toBeDefined());
  await store.switchVaultRoot("obsidian"); await store.openNote("same.md");
  finish(note("same.md", "old reload")); await pending;
  expect(store.content).toBe("obsidian");
});
it("does not let old backlinks replace metadata for the same path in another vault", async () => {
  let finish!: (value: { backlinks: string[] }) => void;
  api.backlinks.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const pending = store.openNote("same.md"); await vi.waitFor(() => expect(finish).toBeDefined());
  await store.switchVaultRoot("obsidian"); await store.openNote("same.md");
  finish({ backlinks: ["wrong-vault.md"] }); await pending;
  expect(store.backlinks).toEqual(["obsidian"]);
});
