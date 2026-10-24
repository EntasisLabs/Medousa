/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ scope: "a", roots: vi.fn(), search: vi.fn(), select: vi.fn() }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => api.scope }));
vi.mock("$lib/daemon", () => ({ listVaultRoots: api.roots, searchVaultNotes: api.search, setActiveVaultRoot: api.select }));
import { VaultSearchCatalog } from "./vaultSearchCatalog.svelte";
beforeEach(() => {
  vi.resetAllMocks(); api.scope = "a";
  api.roots.mockResolvedValue({ roots: [{ id: "personal", label: "Medousa" }, { id: "obsidian", label: "Obsidian" }] });
  api.search.mockImplementation(async (_query, _limit, _tags, root) => ({ root_id: root, hits: [{ note: { path: "same.md", title: root }, score: 1 }] }));
});
it("searches both roots without selecting either and retains identical paths as separate hits", async () => {
  const catalog = new VaultSearchCatalog(); await catalog.search("same");
  expect(api.search).toHaveBeenCalledWith("same", 24, undefined, "personal");
  expect(api.search).toHaveBeenCalledWith("same", 24, undefined, "obsidian");
  expect(catalog.resultsFor("same").map(hit => [hit.rootId, hit.rootLabel, hit.note.path])).toEqual([
    ["personal", "Medousa", "same.md"], ["obsidian", "Obsidian", "same.md"],
  ]);
  expect(api.select).not.toHaveBeenCalled();
});
it("keeps available roots searchable when one fails and names the unavailable vault", async () => {
  api.search.mockImplementation(async (_q, _l, _t, root) => { if (root === "obsidian") throw new Error("Permission denied"); return { root_id: root, hits: [{ note: { path: "same.md" }, score: 1 }] }; });
  const catalog = new VaultSearchCatalog(); await catalog.search("same");
  expect(catalog.resultsFor("same")).toHaveLength(1); expect(catalog.error).toContain("Obsidian");
});
it("deduplicates searches and rejects responses from an old workshop", async () => {
  const catalog = new VaultSearchCatalog();
  let finish!: (value: unknown) => void;
  api.roots.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const first = catalog.search("same"), second = catalog.search("same");
  expect(first).toBe(second); await vi.waitFor(() => expect(finish).toBeDefined());
  api.scope = "b"; finish({ roots: [{ id: "personal", label: "Old" }] }); await first;
  expect(catalog.resultsFor("same")).toEqual([]); expect(api.search).not.toHaveBeenCalled();
});
it("drops stale query results without replacing a newer query", async () => {
  const catalog = new VaultSearchCatalog();
  let finish!: (value: unknown) => void;
  api.search.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const old = catalog.search("old"); await vi.waitFor(() => expect(finish).toBeDefined());
  await catalog.search("new"); finish({ hits: [{ note: { path: "stale.md" }, score: 1 }] }); await old;
  expect(catalog.query).toBe("new"); expect(catalog.resultsFor("new").some(hit => hit.note.path === "stale.md")).toBe(false);
});
it("rejects unscoped results from an older engine instead of mislabeling them", async () => {
  api.search.mockResolvedValue({ hits: [{ note: { path: "wrong.md" }, score: 1 }] });
  const catalog = new VaultSearchCatalog(); await catalog.search("same");
  expect(catalog.resultsFor("same")).toEqual([]);
  expect(catalog.error).toContain("Update the workshop engine");
});
