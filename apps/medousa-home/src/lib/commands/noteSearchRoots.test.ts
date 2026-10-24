/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
import type { WorkshopCommandContext } from "./types";
const api = vi.hoisted(() => ({ scope: "a", refresh: vi.fn(), select: vi.fn(), open: vi.fn(), close: vi.fn() }));
vi.mock("$lib/utils/workshopLocality", async original => ({ ...await original<typeof import("$lib/utils/workshopLocality")>(), activeWorkshopId: () => api.scope }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: { refreshVaultRoots: api.refresh, switchVaultRoot: api.select } }));
vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({ lmeWorkspace: { openNote: api.open } }));
vi.mock("$lib/vault/vaultSearchCatalog.svelte", () => ({ vaultSearchCatalog: {
  get scope() { return api.scope; },
  resultsFor: () => ["Medousa", "Obsidian"].map(root => ({ rootId: root.toLowerCase(), rootLabel: root, note: { path: "same.md", title: "Same title" }, score: 1, snippet: root })),
} }));
import { buildNoteOpenCommands } from "./searchProviders";
const ctx = { callbacks: { close: api.close } } as unknown as WorkshopCommandContext;
beforeEach(() => { vi.resetAllMocks(); api.scope = "a"; });
it("keeps same-path notes distinct and labels every result with its vault", () => {
  const commands = buildNoteOpenCommands(ctx, "same");
  expect(new Set(commands.map(command => command.id)).size).toBe(2);
  expect(commands.map(command => command.subtitle)).toEqual(["Medousa · same.md", "Obsidian · same.md"]);
  expect(commands[1].preview).toEqual({ kind: "text", text: "Obsidian · same.md\n\nObsidian" });
});
it("selects the result's vault before opening it and closes only after success", async () => {
  await buildNoteOpenCommands(ctx, "same")[1].run(ctx);
  expect(api.select).toHaveBeenCalledWith("obsidian"); expect(api.open).toHaveBeenCalledWith("same.md");
  expect(api.refresh.mock.invocationCallOrder[0]).toBeLessThan(api.select.mock.invocationCallOrder[0]);
  expect(api.select.mock.invocationCallOrder[0]).toBeLessThan(api.open.mock.invocationCallOrder[0]);
  expect(api.close).toHaveBeenCalledOnce();
});
it("does not open a same-path note when switching or workshop ownership fails", async () => {
  api.select.mockRejectedValueOnce(new Error("Save pending edits first"));
  await expect(buildNoteOpenCommands(ctx, "same")[1].run(ctx)).rejects.toThrow("Save pending edits");
  expect(api.open).not.toHaveBeenCalled(); expect(api.close).not.toHaveBeenCalled();
  api.refresh.mockImplementationOnce(async () => { api.scope = "b"; });
  await expect(buildNoteOpenCommands(ctx, "same")[1].run(ctx)).rejects.toThrow("workshop changed");
  expect(api.open).not.toHaveBeenCalled();
});
