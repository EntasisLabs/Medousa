import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({
  activate: vi.fn(), close: vi.fn(), hide: vi.fn(), back: vi.fn(), forward: vi.fn(),
  navigate: vi.fn(), query: vi.fn(), reload: vi.fn(), stop: vi.fn(), resolve: vi.fn(), mirror: vi.fn(),
}));
vi.mock("$lib/humanBrowser", () => ({
  humanBrowserActivateTab: native.activate, humanBrowserCloseTab: native.close,
  humanBrowserEmbedHide: native.hide, humanBrowserGoBack: native.back,
  humanBrowserGoForward: native.forward, humanBrowserNavigate: native.navigate,
  humanBrowserQueryNavState: native.query, humanBrowserReload: native.reload,
  humanBrowserStop: native.stop, humanBrowserFindInPage: vi.fn(),
}));
vi.mock("$lib/utils/resolveBrowserDestination", () => ({ resolveBrowserDestination: native.resolve }));
vi.mock("$lib/browser/browserHistory.svelte", () => ({ browserHistory: { record: vi.fn() } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { mirrorWebTab: native.mirror } }));
import { HumanBrowserStore } from "./humanBrowser.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const stores: HumanBrowserStore[] = [];
function store(key = "test-browser") {
  const result = new HumanBrowserStore(key);
  stores.push(result);
  return result;
}
async function page(s: HumanBrowserStore, name: string) {
  await s.openTab(`https://${name}.test/`);
  return s.activeTab!.id;
}

beforeEach(() => {
  vi.useFakeTimers();
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value) });
  for (const mock of Object.values(native)) mock.mockReset();
  native.query.mockImplementation(async (tabId: string) => ({ tabId, canGoBack: false, canGoForward: false }));
  native.resolve.mockImplementation(async (input: string) => input);
});
afterEach(() => { stores.splice(0).forEach((s) => s.dispose()); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("native browser navigation and tab ownership", () => {
  it("uses native Back/Forward without rebuilding history as URL loads", async () => {
    const s = store(); const id = await page(s, "a");
    s.setNativeNavState(true, true, id);
    native.query.mockResolvedValue({ tabId: id, canGoBack: false, canGoForward: true, url: "https://previous.test/", loading: false });
    native.activate.mockClear();
    await s.goBack(); await s.goForward();
    expect(native.back).toHaveBeenCalledWith(id);
    expect(native.forward).toHaveBeenCalledWith(id);
    expect(native.navigate).not.toHaveBeenCalled();
    expect(native.activate).not.toHaveBeenCalled();
    expect(s.activeUrl).toBe("https://previous.test/");
    expect(s.canGoBack).toBe(false);
  });

  it("preserves title and icon for same-document routes", async () => {
    const s = store(); const id = await page(s, "a");
    s.syncFromNative({ tabId: id, url: s.activeUrl, title: "Page title", favicon: "https://a.test/icon.png" });
    s.syncFromNative({ tabId: id, url: "https://a.test/#route", inPage: true });
    expect(s.activeTab?.title).toBe("Page title");
    expect(s.activeTab?.favicon).toBe("https://a.test/icon.png");
    expect(s.activeUrl).toBe("https://a.test/#route");
  });

  it("keeps loading and navigation buttons on their own tabs without a fake timeout", async () => {
    const s = store(); const a = await page(s, "a"); const b = await page(s, "b");
    s.setLoading(false, b); s.setLoading(true, a); s.setNativeNavState(true, false, a);
    await vi.advanceTimersByTimeAsync(10000);
    expect(s.loading).toBe(false); expect(s.canGoBack).toBe(false);
    expect(s.tabs.find((t) => t.id === a)?.loading).toBe(true);
    await s.activateTab(a);
    // Native state is authoritative on activation; a loading event for B must not overwrite A.
    s.setLoading(true, a); s.setLoading(false, b);
    expect(s.loading).toBe(true);
  });

  it("applies a late query only to the tab it requested and coalesces concurrent queries", async () => {
    const s = store(); const a = await page(s, "a");
    const wait = deferred<{ tabId: string; canGoBack: boolean; canGoForward: boolean }>();
    native.query.mockReturnValueOnce(wait.promise);
    const first = s.refreshNativeNavState(a); const second = s.refreshNativeNavState(a);
    const b = await page(s, "b");
    wait.resolve({ tabId: a, canGoBack: true, canGoForward: true }); await first; await second;
    expect(s.activeTab?.id).toBe(b); expect(s.canGoBack).toBe(false);
    expect(s.tabs.find((t) => t.id === a)?.canGoForward).toBe(true);
    expect(native.query.mock.calls.filter(([id]) => id === a)).toHaveLength(2); // activation + coalesced query
  });

  it("never activates another tab when a delayed address lookup completes", async () => {
    const s = store(); const a = await page(s, "a");
    const wait = deferred<string>(); native.resolve.mockReturnValueOnce(wait.promise);
    const navigation = s.navigate("some search"); const b = await page(s, "b");
    native.activate.mockClear(); wait.resolve("https://result.test/"); await navigation;
    expect(s.activeTab?.id).toBe(b);
    expect(native.navigate).toHaveBeenCalledWith("https://result.test/", a);
    expect(native.activate).not.toHaveBeenCalled();
  });

  it("ignores superseded address lookups and Stop cancels a pending lookup", async () => {
    const s = store(); const id = await page(s, "a");
    const old = deferred<string>(); native.resolve.mockReturnValueOnce(old.promise);
    const first = s.navigate("old"); await s.navigate("https://new.test/");
    old.resolve("https://old.test/"); await first;
    expect(s.activeUrl).toBe("https://new.test/");
    const stopped = deferred<string>(); native.resolve.mockReturnValueOnce(stopped.promise);
    const last = s.navigate("stop me"); await s.stop(); stopped.resolve("https://stopped.test/"); await last;
    expect(s.activeUrl).toBe("https://new.test/"); expect(s.loading).toBe(false);
    native.query.mockClear(); await s.refreshNativeNavState(); expect(native.query).toHaveBeenCalledWith(id);
  });

  it("does not hide the main browser when a pop-out opens a blank tab", async () => {
    const s = store("medousa-browser-session-popout");
    await s.openTab();
    expect(native.hide).not.toHaveBeenCalled();
  });

  it("does not reload on ordinary tab activation", async () => {
    const s = store(); const a = await page(s, "a"); await page(s, "b");
    native.activate.mockClear(); await s.activateTab(a);
    expect(native.activate).toHaveBeenCalledWith(a, "https://a.test/");
    expect(native.reload).not.toHaveBeenCalled(); expect(native.navigate).not.toHaveBeenCalled();
  });

  it("does not steal selection when a queued reload belongs to a previous tab", async () => {
    const s = store(); const a = await page(s, "a");
    const reload = s.reload(); const opening = s.openTab("https://b.test/"); const b = s.activeTab!.id;
    await reload; await opening;
    expect(s.activeTab?.id).toBe(b); expect(native.reload).toHaveBeenCalledWith(a);
    expect(native.activate.mock.calls.at(-1)?.[0]).toBe(b);
  });

  it("removes closed tabs immediately and ignores late native results", async () => {
    const s = store(); const a = await page(s, "a");
    const wait = deferred<void>(); native.close.mockReturnValueOnce(wait.promise);
    const closing = s.closeTab(a);
    expect(s.tabs.some((t) => t.id === a)).toBe(false);
    s.syncFromNative({ tabId: a, url: "https://late.test/" }); s.setLoading(true, a);
    expect(s.activeUrl).not.toBe("https://late.test/");
    await s.closeTab(a); wait.resolve(); await closing;
    expect(native.close).toHaveBeenCalledTimes(1);
  });

  it("selects an adjacent tab on close and preserves selection for background closes", async () => {
    const s = store(); const a = await page(s, "a"); const b = await page(s, "b"); const c = await page(s, "c");
    await s.activateTab(b); await s.closeTab(b); expect(s.activeTab?.id).toBe(c);
    await s.closeTab(a); expect(s.activeTab?.id).toBe(c);
    await s.closeTab(c); expect(s.activeUrl).toBe("about:blank");
  });

  it("keeps a single blank tab after the last close and rolls back a failed native close", async () => {
    const s = store(); await s.closeTab(s.activeTab!.id);
    expect(s.tabs).toHaveLength(1); expect(s.activeUrl).toBe("about:blank");
    const id = await page(s, "a"); native.close.mockRejectedValueOnce(new Error("close failed"));
    await s.closeTab(id);
    expect(s.tabs.find((t) => t.id === id)?.error).toBe("close failed");
    expect(s.closedTabs.some((t) => t.id === id)).toBe(false);
  });

  it("opens more than twelve tabs and reopens closed metadata with a fresh native identity", async () => {
    const s = store(); for (let i = 0; i < 13; i++) await page(s, `page${i}`);
    expect(s.tabs).toHaveLength(14);
    const id = s.activeTab!.id; s.syncFromNative({ tabId: id, url: s.activeUrl, title: "Saved title" });
    await s.closeTab(id); native.activate.mockClear(); await s.reopenClosedTab();
    expect(s.activeTab?.id).not.toBe(id); expect(s.activeTab?.title).toBe("Saved title");
    expect(native.activate).toHaveBeenCalledTimes(1);
  });

  it("does not let an unused store in another window overwrite the owning window's session", async () => {
    const owner = store("shared-session");
    const unused = store("shared-session");
    const id = await page(owner, "a"); await owner.closeTab(id);
    owner.dispose();
    const saved = localStorage.getItem("shared-session");
    unused.dispose();
    expect(localStorage.getItem("shared-session")).toBe(saved);
    const restored = store("shared-session");
    expect(restored.closedTabs[0]?.url).toBe("https://a.test/");
  });

  it("persists sessions independently and restores metadata without stale fake history", async () => {
    const a = store("a"); const b = store("b"); await page(a, "a"); await page(b, "b");
    a.setNativeNavState(true, true); a.setLoading(true);
    await vi.advanceTimersByTimeAsync(200);
    expect(JSON.parse(localStorage.getItem("a")!).tabs.at(-1)).not.toHaveProperty("loading");
    const restored = store("a"); expect(restored.activeUrl).toBe("https://a.test/");
    expect(restored.loading).toBe(false); expect(restored.canGoBack).toBe(false);
    expect(JSON.parse(localStorage.getItem("b")!).tabs.at(-1).url).toBe("https://b.test/");
  });
});
