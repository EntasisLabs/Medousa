import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("$lib/daemon", () => ({
  addVaultRoot: vi.fn(),
  createVaultNote: vi.fn(),
  deleteVaultNote: vi.fn(),
  getVaultBacklinks: vi.fn(async () => ({ backlinks: [] })),
  getVaultNote: vi.fn(),
  listVaultNotes: vi.fn(async () => ({ notes: [] })),
  listVaultRoots: vi.fn(async () => ({ roots: [] })),
  listVaultTags: vi.fn(async () => ({ tags: [] })),
  saveVaultNote: vi.fn(),
  searchVaultNotes: vi.fn(async () => ({ hits: [] })),
  setActiveVaultRoot: vi.fn(),
}));

function noteResponse(path: string, content: string, title = "Note", kind = "note") {
  return {
    content,
    note: {
      path,
      title,
      kind,
      content_hash: `hash-${path}`,
      wikilinks_out: [],
      backlinks: [],
      tags: [],
    },
  };
}

describe("vault note kind surfaces (sheet / workbook / slides)", () => {
  vi.setConfig({ testTimeout: 20_000 });

  beforeEach(() => {
    vi.resetModules();
    const storage = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      },
      clear: () => storage.clear(),
    });
  });

  async function loadStore() {
    const { VaultStore } = await import("./vault.svelte");
    const { getVaultNote } = await import("$lib/daemon");
    const mocked = vi.mocked(getVaultNote);
    mocked.mockReset();
    return { store: new VaultStore(), getVaultNote: mocked };
  }

  it("setNoteKind(sheet) seeds a table and stays sheet after buffer restore", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote.mockImplementation(async (path: string) =>
      noteResponse(path, `# ${path}\n`, "Note", "note") as never,
    );

    await store.openNote("notes/a.md");
    store.setNoteKind("sheet");
    expect(store.selectedKind).toBe("sheet");
    expect(store.content).toMatch(/\| --- \|/);
    expect(store.ledgerEditMode).toBe("table");

    await store.openNote("notes/b.md");
    expect(store.contentFor("notes/a.md")).toMatch(/kind:\s*sheet/);

    await store.openNote("notes/a.md");
    expect(store.selectedKind).toBe("sheet");
    expect(store.ledgerEditMode).toBe("table");
  });

  it("setNoteKind(workbook) seeds marker and opens workbook view mode", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote.mockResolvedValue(
      noteResponse("workbooks/Ops/_workbook.md", "", "Ops", "note") as never,
    );

    await store.openNote("workbooks/Ops/_workbook.md");
    store.setNoteKind("workbook");
    expect(store.selectedKind).toBe("workbook");
    expect(store.workbookEditMode).toBe("view");
    expect(store.content).toContain("kind: workbook");
    expect(store.content).toContain("sheets:");
  });

  it("setNoteKind(slides) keeps deck mode (not Live default)", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote.mockResolvedValue(
      noteResponse("decks/talk.md", "# Talk\n", "Talk", "note") as never,
    );

    await store.openNote("decks/talk.md");
    store.setNoteKind("slides");
    expect(store.selectedKind).toBe("slides");
    expect(store.deckEditMode).toBe("deck");
    expect(store.content).toMatch(/kind:\s*slides/);
  });

  it("does not wipe an existing sheet table when re-applying kind", async () => {
    const { store, getVaultNote } = await loadStore();
    const body = [
      "---",
      "kind: sheet",
      "---",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
    ].join("\n");
    getVaultNote.mockResolvedValue(
      noteResponse("notes/grid.md", body, "Grid", "sheet") as never,
    );

    await store.openNote("notes/grid.md");
    expect(store.selectedKind).toBe("sheet");
    store.setNoteKind("sheet");
    expect(store.content.match(/\| --- \| --- \|/g)?.length).toBe(1);
    expect(store.content).toContain("| 1 | 2 |");
  });
});
