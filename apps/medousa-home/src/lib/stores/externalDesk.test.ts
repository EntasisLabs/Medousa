import { describe, expect, it, vi } from "vitest";
const fixture = vi.hoisted(() => ({ scan: vi.fn() }));
vi.mock("$lib/utils/externalDeskApi", () => ({ scanExternalRoot: fixture.scan, pickExternalFolder: vi.fn(), rootLabelFromPath: (path: string) => path }));
import { ExternalDeskStore } from "./externalDesk.svelte";

describe("file refresh requests", () => {
  it("coalesces a folder scan while allowing another folder to complete", async () => {
    let finishSlow!: (value: []) => void;
    fixture.scan.mockImplementation((path: string) => path === "/slow" ? new Promise((resolve) => { finishSlow = resolve; }) : Promise.resolve([]));
    const store = new ExternalDeskStore();
    store.pinnedRoots = [{ id: "slow", label: "Slow", path: "/slow" }, { id: "fast", label: "Fast", path: "/fast" }];
    const first = store.refreshRoot("/slow");
    const duplicate = store.refreshRoot("/slow");
    await store.refreshRoot("/fast");
    expect(fixture.scan.mock.calls.map(([path]) => path)).toEqual(["/slow", "/fast"]);
    expect(store.entriesByRoot["/fast"]).toEqual([]);
    finishSlow([]); await Promise.all([first, duplicate]);
  });
});
