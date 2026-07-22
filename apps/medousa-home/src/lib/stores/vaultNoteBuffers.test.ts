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

vi.mock("$lib/utils/vaultFilesystem", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/utils/vaultFilesystem")>();
  return {
    ...actual,
    readAbsoluteTextFile: vi.fn(),
    writeAbsoluteTextFile: vi.fn(),
    pickMarkdownFile: vi.fn(),
  };
});

function noteResponse(path: string, content: string, title = "Note") {
  return {
    content,
    created: true,
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
  // VaultStore pulls a large editor/codec graph; cold dynamic import is slow.
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

  it("restores a clean stashed buffer instead of refetching", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote.mockResolvedValue(noteResponse("notes/a.md", "fresh") as never);

    store.seedBufferForTest({
      path: "notes/a.md",
      content: "stashed",
      baselineContent: "stashed",
      contentHash: "hash-a",
      title: "A",
      dirty: false,
      contentRevision: 1,
    });

    await store.openNote("notes/a.md");
    expect(store.content).toBe("stashed");
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

  it("sets selectedPath before applying cold-open body (handoff)", async () => {
    const { store, getVaultNote } = await loadStore();
    let pathDuringFetch: string | null = null;
    getVaultNote.mockImplementation(async (path: string) => {
      pathDuringFetch = store.selectedPath;
      return noteResponse(path, `body:${path}`, path === "notes/b.md" ? "Bravo" : "Alpha") as never;
    });

    await store.openNote("notes/a.md");
    await store.openNote("notes/b.md");

    expect(pathDuringFetch).toBe("notes/b.md");
    expect(store.selectedPath).toBe("notes/b.md");
    expect(store.content).toBe("body:notes/b.md");
    expect(store.title).toBe("Bravo");
    // Previous note preserved in buffer — not overwritten by B's template.
    expect(store.contentFor("notes/a.md")).toBe("body:notes/a.md");
  });

  it("drops stale overlapping openNote fetches", async () => {
    const { store, getVaultNote } = await loadStore();
    let releaseA!: (value: unknown) => void;
    const holdA = new Promise((resolve) => {
      releaseA = resolve;
    });

    getVaultNote.mockImplementation(async (path: string) => {
      if (path === "notes/a.md") {
        await holdA;
        return noteResponse(path, "stale-a", "Stale A") as never;
      }
      return noteResponse(path, "fresh-b", "Fresh B") as never;
    });

    const openA = store.openNote("notes/a.md");
    await store.openNote("notes/b.md");
    releaseA(undefined);
    await openA;

    expect(store.selectedPath).toBe("notes/b.md");
    expect(store.content).toBe("fresh-b");
    expect(store.title).toBe("Fresh B");
  });

  it("rejects markDirty from a non-focused path", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote.mockImplementation(async (path: string) =>
      noteResponse(path, `body:${path}`) as never,
    );

    await store.openNote("notes/a.md");
    store.markDirty("hijack", { path: "notes/b.md" });
    expect(store.content).toBe("body:notes/a.md");
    expect(store.dirty).toBe(false);

    store.markDirty("legit edit", { path: "notes/a.md" });
    expect(store.content).toBe("legit edit");
    expect(store.dirty).toBe(true);
  });

  it("failed cold open clears body and allows retry refetch", async () => {
    const { store, getVaultNote } = await loadStore();
    getVaultNote
      .mockResolvedValueOnce(noteResponse("notes/a.md", "alpha", "Alpha") as never)
      .mockRejectedValueOnce(new Error("boom"))
      .mockResolvedValueOnce(noteResponse("notes/b.md", "bravo", "Bravo") as never);

    await store.openNote("notes/a.md");
    await store.openNote("notes/b.md");

    expect(store.selectedPath).toBe("notes/b.md");
    expect(store.content).toBe("");
    expect(store.contentHash).toBeNull();
    expect(store.error).toMatch(/boom/);

    await store.openNote("notes/b.md");
    expect(store.content).toBe("bravo");
    expect(store.title).toBe("Bravo");
    expect(getVaultNote).toHaveBeenCalledTimes(3);
  });

  it("serves absolute loose paths without vault-normalizing contentFor", async () => {
    const { store } = await loadStore();
    store.looseFilePath = "/tmp/loose-note.md";
    store.selectedPath = "/tmp/loose-note.md";
    store.content = "loose body";
    store.title = "loose-note";
    store.contentHash = null;
    store.dirty = false;

    expect(store.isFocusedPath("/tmp/loose-note.md")).toBe(true);
    expect(store.contentFor("/tmp/loose-note.md")).toBe("loose body");
    expect(store.contentSyncKeyFor("/tmp/loose-note.md")).toBe(store.contentSyncKey);
    // Vault-normalized form must not steal the focused absolute body.
    expect(store.contentFor("tmp/loose-note.md")).toBe("");
  });

  it("stashes and restores absolute loose buffers across focus switches", async () => {
    const { store } = await loadStore();
    const { readAbsoluteTextFile, writeAbsoluteTextFile } = await import(
      "$lib/utils/vaultFilesystem"
    );
    const readMock = vi.mocked(readAbsoluteTextFile);
    const writeMock = vi.mocked(writeAbsoluteTextFile);
    writeMock.mockResolvedValue(undefined as never);
    readMock.mockImplementation(async (path: string) => {
      if (path === "/tmp/a.md") return "# Alpha loose";
      if (path === "/tmp/b.md") return "# Bravo loose";
      throw new Error(`unexpected path ${path}`);
    });

    expect(await store.openLooseFile("/tmp/a.md")).toBe(true);
    expect(store.content).toBe("# Alpha loose");
    store.markDirty("# Alpha loose — edited", { path: "/tmp/a.md" });

    expect(await store.openLooseFile("/tmp/b.md")).toBe(true);
    expect(store.content).toBe("# Bravo loose");
    // Leave flush autosaves absolute files, then keeps the body in the buffer.
    expect(writeMock).toHaveBeenCalledWith("/tmp/a.md", "# Alpha loose — edited");
    expect(store.contentFor("/tmp/a.md")).toBe("# Alpha loose — edited");

    expect(await store.openLooseFile("/tmp/a.md")).toBe(true);
    expect(store.content).toBe("# Alpha loose — edited");
    // Buffer-first reopen should not re-read disk for a.
    expect(readMock.mock.calls.filter((c) => c[0] === "/tmp/a.md")).toHaveLength(1);
  });

  it("failed loose open clears body and does not leave a blocking buffer", async () => {
    const { store } = await loadStore();
    const { readAbsoluteTextFile } = await import("$lib/utils/vaultFilesystem");
    const readMock = vi.mocked(readAbsoluteTextFile);
    readMock
      .mockRejectedValueOnce(new Error("enoent"))
      .mockResolvedValueOnce("# recovered");

    expect(await store.openLooseFile("/tmp/missing.md")).toBe(false);
    expect(store.content).toBe("");
    expect(store.error).toMatch(/enoent/);
    expect(store.contentFor("/tmp/missing.md")).toBe("");

    expect(await store.openLooseFile("/tmp/missing.md")).toBe(true);
    expect(store.content).toBe("# recovered");
  });

  it("never vault-normalizes absolute paths away on openLooseFile", async () => {
    const { store } = await loadStore();
    const { readAbsoluteTextFile } = await import("$lib/utils/vaultFilesystem");
    vi.mocked(readAbsoluteTextFile).mockResolvedValue("body");

    await store.openLooseFile("/tmp/loose-note.md");
    expect(store.selectedPath).toBe("/tmp/loose-note.md");
    expect(store.looseFilePath).toBe("/tmp/loose-note.md");
    expect(store.isLooseFile).toBe(true);
  });

  it("createNote handoff does not leave prior path dirty with new template", async () => {
    const { store } = await loadStore();
    const { createVaultNote, getVaultNote, saveVaultNote } = await import("$lib/daemon");
    const createMock = vi.mocked(createVaultNote);
    const getMock = vi.mocked(getVaultNote);
    const saveMock = vi.mocked(saveVaultNote);
    saveMock.mockResolvedValue({
      content: "template-b",
      note: {
        path: "notes/a.md",
        title: "Wiped",
        kind: "note",
        content_hash: "hash-bad",
        wikilinks_out: [],
        backlinks: [],
        tags: [],
      },
    } as never);

    getMock.mockImplementation(async (path: string) => {
      if (path === "notes/a.md") return noteResponse(path, "hours of work", "Active") as never;
      return noteResponse(path, "template-b", "New Note") as never;
    });
    createMock.mockResolvedValue(
      noteResponse("notes/new-note.md", "template-b", "New Note") as never,
    );

    await store.openNote("notes/a.md");
    store.markDirty("hours of work — edited", { path: "notes/a.md" });
    // Simulate leave-flush success without actually waiting on timers.
    saveMock.mockResolvedValueOnce({
      content: "hours of work — edited",
      note: {
        path: "notes/a.md",
        title: "Active",
        kind: "note",
        content_hash: "hash-a2",
        wikilinks_out: [],
        backlinks: [],
        tags: [],
      },
    } as never);

    await store.createNote({
      spaceId: "other",
      title: "New Note",
      path: "notes/new-note.md",
      content: "template-b",
    });

    expect(store.selectedPath).toBe("notes/new-note.md");
    expect(store.content).toBe("template-b");
    // Prior note buffer must still hold the edited body, not the new template.
    expect(store.contentFor("notes/a.md")).toBe("hours of work — edited");
    const wipedPuts = saveMock.mock.calls.filter(
      (call) => call[0] === "notes/a.md" && call[1] === "template-b",
    );
    expect(wipedPuts).toHaveLength(0);
  });

  it("createNote refuses an existing path without calling createVaultNote", async () => {
    const { store } = await loadStore();
    const { createVaultNote, listVaultNotes, getVaultNote } = await import("$lib/daemon");
    const createMock = vi.mocked(createVaultNote);
    const listMock = vi.mocked(listVaultNotes);
    const getMock = vi.mocked(getVaultNote);

    listMock.mockResolvedValue({
      notes: [
        {
          path: "notes/taken.md",
          title: "Taken",
          kind: "note",
          content_hash: "hash-taken",
          wikilinks_out: [],
          backlinks: [],
          tags: [],
        },
      ],
    } as never);
    getMock.mockResolvedValue(noteResponse("notes/taken.md", "keep this body", "Taken") as never);

    await store.refreshNotes();
    await store.openNote("notes/other.md");
    store.markDirty("other draft", { path: "notes/other.md" });
    createMock.mockClear();

    const result = await store.createNote({
      spaceId: "other",
      title: "Taken",
      path: "notes/taken.md",
      content: "# Wiped template\n",
    });

    expect(result).toBeNull();
    expect(store.error).toMatch(/already exists/i);
    expect(createMock).not.toHaveBeenCalled();
    expect(store.contentFor("notes/other.md")).toBe("other draft");
    expect(store.selectedPath).toBe("notes/other.md");
  });
});
