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

function noteResponse(path: string, content: string, title = "Note") {
  return {
    content,
    note: {
      path,
      title,
      kind: "note",
      content_hash: `hash-${path}`,
      wikilinks_out: [],
      backlinks: [],
      tags: [],
    },
  };
}

describe("vault note buffers (multi-pane)", () => {
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
    mocked.mockImplementation(async (path: string) =>
      noteResponse(path, `body:${path}`) as never,
    );
    return { store: new VaultStore(), getVaultNote: mocked };
  }

  it("keeps demoted note content when focusing another path", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote.mockImplementation(async (path: string) =>
      noteResponse(path, path === "notes/a.md" ? "alpha" : "bravo") as never,
    );

    await store.openNote("notes/a.md");
    expect(store.contentFor("notes/a.md")).toBe("alpha");

    await store.openNote("notes/b.md");
    expect(store.selectedPath).toBe("notes/b.md");
    expect(store.content).toBe("bravo");
    expect(store.contentFor("notes/a.md")).toBe("alpha");
    expect(store.contentFor("notes/b.md")).toBe("bravo");
  });

  it("restores a dirty buffer instead of refetching", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote.mockResolvedValue(noteResponse("notes/a.md", "fresh") as never);

    store.seedBufferForTest({
      path: "notes/a.md",
      content: "dirty draft",
      baselineContent: "baseline",
      contentHash: "hash-old",
      title: "Draft",
      dirty: true,
      contentRevision: 3,
    });

    await store.openNote("notes/a.md");
    expect(store.content).toBe("dirty draft");
    expect(store.dirty).toBe(true);
    expect(getVaultNote).not.toHaveBeenCalled();
  });

  it("warms a background buffer without changing focus", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote
      .mockResolvedValueOnce(noteResponse("notes/a.md", "alpha") as never)
      .mockResolvedValueOnce(noteResponse("notes/b.md", "bravo") as never);

    await store.openNote("notes/a.md");
    await store.warmBuffer("notes/b.md");

    expect(store.selectedPath).toBe("notes/a.md");
    expect(store.content).toBe("alpha");
    expect(store.contentFor("notes/b.md")).toBe("bravo");
  });
});
