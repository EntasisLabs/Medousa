import { it, expect, vi, beforeEach } from "vitest";
const { request } = vi.hoisted(() => ({ request: vi.fn() }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: request }));
import { CreateBrowser } from "./createBrowser.svelte";
beforeEach(() => request.mockReset());
it("keeps save/delete receipts when an earlier list response arrives late", async () => {
  let finish!: (value: unknown) => void;
  request.mockImplementationOnce(() => new Promise(resolve => finish = resolve));
  const browser = new CreateBrowser("receipts");
  const loading = browser.load();
  const doc = { id: "new", title: "Renamed", pinned: true, revision: 2, updated_at: "now" };
  browser.acknowledge(doc as never);
  browser.forget("deleted");
  finish({ projects: [{ ...doc, title: "Old", revision: 0 }, { ...doc, id: "deleted" }], next_cursor: null });
  await loading;
  expect(browser.projects).toEqual([doc]);
  browser.acknowledge({ ...doc, title: "Stale", revision: 1 } as never);
  browser.acknowledge({ ...doc, id: "deleted" } as never);
  expect(browser.projects).toEqual([doc]);
});
it("retries a new project with the same document ID, scene and starter after a lost response", async () => {
  let fail = true;
  request.mockImplementation(async (_op, _params, body) => {
    if (fail) {
      fail = false;
      throw new Error("lost reply");
    }
    return {
      ...body,
      revision: 0,
      pinned: false,
      created_at: "now",
      updated_at: "now",
    };
  });
  const browser = new CreateBrowser("workshop");
  await expect(browser.create("composition")).rejects.toThrow("lost reply");
  const first = structuredClone(request.mock.calls[0][2]);
  await browser.create("blank");
  expect(request.mock.calls[1][2]).toEqual(first);
  expect(browser.projects[0].title).toBe("Image composition");
  expect(browser.pending).toBeNull();
});
