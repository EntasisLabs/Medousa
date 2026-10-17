/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it, vi } from "vitest";
const fixture = vi.hoisted(() => ({ list: vi.fn(), local: true }));
vi.mock("$lib/utils/externalDeskApi", () => ({ listExternalDirectory: fixture.list, pickExternalFolder: vi.fn(), rootLabelFromPath: (path: string) => path }));
vi.mock("$lib/utils/workshopLocality", () => ({ isCoLocatedWorkshop: () => fixture.local, vaultPinFolderRemoteHint: () => "Remote" }));
import { ExternalDeskStore } from "./externalDesk.svelte";
const file = (path: string, is_dir = false) => ({ path, name: path.split("/").pop()!, is_dir, modified_at_utc: "", size_bytes: 0 });
function setup() {
  const store = new ExternalDeskStore();
  store.pinnedRoots = [{ id: "slow", label: "Slow", path: "/slow" }, { id: "fast", label: "Fast", path: "/fast" }];
  return store;
}
beforeEach(() => { vi.clearAllMocks(); localStorage.clear(); fixture.local = true; fixture.list.mockResolvedValue({ entries: [], next_cursor: null }); });
describe("folder browsing", () => {
  it("does not scan collapsed folders on entering Files", async () => {
    const store = setup(); store.setSidebarMode("files"); await store.refreshAllRoots();
    expect(fixture.list).not.toHaveBeenCalled();
  });
  it("coalesces a folder listing while another folder completes independently", async () => {
    let finishSlow!: (value: object) => void;
    fixture.list.mockImplementation((path: string) => path === "/slow" ? new Promise((resolve) => { finishSlow = resolve; }) : Promise.resolve({ entries: [], next_cursor: null }));
    const store = setup(); const first = store.loadDirectory("/slow"); const duplicate = store.loadDirectory("/slow");
    await store.loadDirectory("/fast");
    expect(fixture.list.mock.calls.map(([path]) => path)).toEqual(["/slow", "/fast"]);
    expect(store.directories["/fast"].loading).toBe(false);
    expect(store.loadingRoot).toBe("/slow");
    finishSlow({ entries: [], next_cursor: null }); await Promise.all([first, duplicate]);
    expect(store.loadingRoot).toBeNull();
  });
  it("drops listings when collapsed and ignores their late responses", async () => {
    let finish!: (value: object) => void;
    fixture.list.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const store = setup(); store.togglePinExpanded("slow"); const pending = store.loadDirectory("/slow");
    store.togglePinExpanded("slow");
    finish({ entries: [file("/slow/late.txt")], next_cursor: null }); await pending;
    expect(store.directories["/slow"]).toBeUndefined();
    expect(store.allEntries).toEqual([]);
  });
  it("ignores a previous hidden-file mode's late response", async () => {
    let finish!: (value: object) => void;
    fixture.list.mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    const store = setup(); const old = store.loadDirectory("/slow"); store.setShowHidden(true);
    fixture.list.mockResolvedValue({ entries: [file("/slow/.env")], next_cursor: null });
    await store.loadDirectory("/slow"); finish({ entries: [], next_cursor: null }); await old;
    expect(store.directories["/slow"].entries[0].name).toBe(".env");
    expect(fixture.list).toHaveBeenLastCalledWith("/slow", null, true);
  });
  it("pages forward and back, releases off-page subfolders, and preserves the current page on remount", async () => {
    fixture.list.mockImplementation(async (path: string, cursor: string | null) => ({ entries: path.endsWith("/child") ? [file(`${path}/nested.txt`)] : cursor ? [file(`${path}/last.txt`)] : [file(`${path}/child`, true)], next_cursor: !cursor && !path.endsWith("/child") ? "next" : null }));
    const store = setup(); await store.loadDirectory("/slow"); await store.loadDirectory("/slow/child");
    await store.changeDirectoryPage("/slow", "next");
    expect(store.directories["/slow/child"]).toBeUndefined();
    expect(store.directories["/slow"].entries[0].name).toBe("last.txt");
    const calls = fixture.list.mock.calls.length; await store.loadDirectory("/slow");
    expect(fixture.list).toHaveBeenCalledTimes(calls);
    await store.changeDirectoryPage("/slow", "previous");
    expect(store.directories["/slow"].entries[0].name).toBe("child");
    expect(store.directories["/slow"].previous).toEqual([]);
  });
  it("keeps a folder's read error separate from other folders and retries it", async () => {
    const store = setup(); fixture.list.mockRejectedValueOnce(new Error("Permission denied"));
    await store.loadDirectory("/slow"); await store.loadDirectory("/fast");
    expect(store.directories["/slow"].error).toBe("Permission denied");
    expect(store.directories["/fast"].error).toBeNull();
    await store.loadDirectory("/slow", true); expect(store.directories["/slow"].error).toBeNull();
  });
  it("never reads the Home machine's directories for a remote workshop", async () => {
    fixture.local = false; const store = setup(); await store.loadDirectory("/slow");
    expect(fixture.list).not.toHaveBeenCalled();
  });
});
