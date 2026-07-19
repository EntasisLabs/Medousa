import { beforeEach, describe, expect, it, vi } from "vitest";

const openNote = vi.fn(async (_path: string) => {});
const previewAttachment = vi.fn();
const closeAttachmentPreview = vi.fn();
const setSidebarMode = vi.fn();
const selectExternalPath = vi.fn();
const openScriptById = vi.fn(async (_id: string) => {});
const openNewTab = vi.fn();
const selectTab = vi.fn();
const closeTab = vi.fn();
const selectArtifact = vi.fn();

const scriptTabs: Array<{ tabId: string; scriptId: string | null; name: string }> = [];
let activeTabId: string | null = null;

vi.mock("$lib/stores/vault.svelte", () => ({
  vault: {
    selectedPath: null as string | null,
    labelByPathMap: new Map<string, string>([["notes/a.md", "Alpha"]]),
    previewingAttachmentPath: null as string | null,
    openNote,
    previewAttachment,
    closeAttachmentPreview,
  },
}));

vi.mock("$lib/stores/graphemeScriptEditor.svelte", () => ({
  graphemeScriptEditor: {
    get tabs() {
      return scriptTabs;
    },
    get activeTabId() {
      return activeTabId;
    },
    set activeTabId(value: string | null) {
      activeTabId = value;
    },
    get activeTab() {
      return scriptTabs.find((tab) => tab.tabId === activeTabId) ?? null;
    },
    openNewTab: () => {
      openNewTab();
      const tab = { tabId: "script-1", scriptId: null, name: "Untitled 1" };
      scriptTabs.push(tab);
      activeTabId = tab.tabId;
    },
    openScriptById: async (id: string) => {
      await openScriptById(id);
      const tab = { tabId: "script-2", scriptId: id, name: "Hello" };
      scriptTabs.push(tab);
      activeTabId = tab.tabId;
    },
    selectTab: (tabId: string) => {
      selectTab(tabId);
      activeTabId = tabId;
    },
    closeTab: (tabId: string) => {
      closeTab(tabId);
      const idx = scriptTabs.findIndex((tab) => tab.tabId === tabId);
      if (idx >= 0) scriptTabs.splice(idx, 1);
      if (activeTabId === tabId) {
        activeTabId = scriptTabs.at(-1)?.tabId ?? null;
      }
    },
  },
}));

vi.mock("$lib/stores/externalDesk.svelte", () => ({
  externalDesk: { setSidebarMode, selectExternalPath },
}));

vi.mock("$lib/stores/artifacts.svelte", () => ({
  artifacts: {
    artifacts: [{ artifact_id: "deck-1", label: "Pitch", session_id: "s1" }],
    selectArtifact,
  },
}));

const { LmeWorkspaceStore } = await import("$lib/stores/lmeWorkspace.svelte");

describe("lmeWorkspace", () => {
  let store: InstanceType<typeof LmeWorkspaceStore>;

  beforeEach(() => {
    scriptTabs.length = 0;
    activeTabId = null;
    store = new LmeWorkspaceStore();
    store.tabs = [];
    store.activeTabId = null;
    store.explorerMode = "notes";
    vi.clearAllMocks();
  });

  it("opens notes as workspace tabs", async () => {
    await store.openNote("notes/a.md");
    expect(openNote).toHaveBeenCalledWith("notes/a.md");
    expect(store.tabs).toHaveLength(1);
    expect(store.tabs[0]).toMatchObject({
      kind: "note",
      path: "notes/a.md",
      title: "Alpha",
    });
    expect(store.explorerMode).toBe("notes");
  });

  it("reuses an existing note tab", async () => {
    await store.openNote("notes/a.md");
    await store.openNote("notes/a.md");
    expect(store.tabs).toHaveLength(1);
    expect(openNote).toHaveBeenCalledTimes(2);
  });

  it("maps automations sections onto explorer modes", () => {
    store.openAutomationsSection("flows");
    expect(store.explorerMode).toBe("flows");
  });

  it("mirrors script editor tabs into the LME strip", async () => {
    await store.openScriptById("s1");
    expect(openScriptById).toHaveBeenCalledWith("s1");
    expect(store.tabs.some((tab) => tab.kind === "script")).toBe(true);
    expect(store.explorerMode).toBe("scripts");
  });

  it("syncScriptTabFromEditor is idempotent without activate", async () => {
    await store.openScriptById("s1");
    const before = store.tabs;
    store.syncScriptTabFromEditor({ activate: false });
    expect(store.tabs).toBe(before);
  });

  it("hydrate openNote does not force explorer mode or steal active tab", async () => {
    await store.openScriptById("s1");
    const scriptTabId = store.activeTabId;
    store.setExplorerMode("scripts");
    await store.openNote("notes/a.md", { activateMode: false });
    expect(store.explorerMode).toBe("scripts");
    expect(store.activeTabId).toBe(scriptTabId);
    expect(store.tabs.some((tab) => tab.kind === "note")).toBe(true);
  });

  it("mode switch does not clear the active script tab", async () => {
    await store.openScriptById("s1");
    const scriptTabId = store.activeTabId;
    store.setExplorerMode("notes");
    store.setExplorerMode("files");
    store.setExplorerMode("scripts");
    expect(store.activeTabId).toBe(scriptTabId);
    expect(store.tabs.some((tab) => tab.kind === "script")).toBe(true);
  });

  it("closing the last script tab empties the strip", async () => {
    await store.openScriptById("s1");
    const tabId = store.activeTabId;
    expect(tabId).toBeTruthy();
    await store.closeTab(tabId!);
    expect(store.tabs).toHaveLength(0);
    expect(store.activeTabId).toBeNull();
    store.syncScriptTabFromEditor({ activate: false });
    expect(store.tabs).toHaveLength(0);
  });

  it("opens external files as tabs", () => {
    store.openFile("/Users/me/doc.pdf");
    expect(store.explorerMode).toBe("files");
    expect(store.tabs[0]).toMatchObject({
      kind: "file",
      path: "/Users/me/doc.pdf",
      title: "doc.pdf",
    });
    expect(previewAttachment).toHaveBeenCalledWith("/Users/me/doc.pdf", "pane");
  });

  it("opens decks as tabs", () => {
    store.openDeck("deck-1", "Pitch");
    expect(store.explorerMode).toBe("presentations");
    expect(store.tabs[0]).toMatchObject({
      kind: "deck",
      artifactId: "deck-1",
      title: "Pitch",
    });
    expect(selectArtifact).toHaveBeenCalledWith("deck-1");
  });
});
