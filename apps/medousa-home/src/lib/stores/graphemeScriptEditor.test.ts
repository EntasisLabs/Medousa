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
});
