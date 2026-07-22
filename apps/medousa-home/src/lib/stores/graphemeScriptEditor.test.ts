import { beforeEach, describe, expect, it } from "vitest";
import { GraphemeScriptEditorStore } from "./graphemeScriptEditor.svelte";

describe("graphemeScriptEditor", () => {
  let store: GraphemeScriptEditorStore;

  beforeEach(() => {
    store = new GraphemeScriptEditorStore();
  });

  it("allows closing the last tab without forcing a replacement", () => {
    store.openNewTab();
    expect(store.tabs).toHaveLength(1);
    const tabId = store.activeTabId!;
    store.closeTab(tabId);
    expect(store.tabs).toHaveLength(0);
    expect(store.activeTabId).toBeNull();
  });

  it("opens highlight-only snippet tabs with languageId", () => {
    store.openLanguageSnippet({
      languageId: "markdown",
      name: "Notes",
      body: "# Hello",
    });
    expect(store.activeTab?.languageId).toBe("markdown");
    expect(store.activeTab?.dirty).toBe(true);
    expect(store.statusMessage).toContain("highlight only");
  });

  it("opens stub languages with preview status", () => {
    store.openLanguageSnippet({ languageId: "python", body: "print('hi')" });
    expect(store.activeTab?.languageId).toBe("python");
    expect(store.statusMessage).toContain("not wired yet");
  });
});
