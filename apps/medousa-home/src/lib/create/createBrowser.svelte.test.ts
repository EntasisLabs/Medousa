import { it, expect, vi } from "vitest";
const { request } = vi.hoisted(() => ({ request: vi.fn() }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: request }));
import { CreateBrowser } from "./createBrowser.svelte";
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
