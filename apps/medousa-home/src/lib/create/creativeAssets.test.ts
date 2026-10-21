/** @vitest-environment happy-dom */
import { it, expect, vi, afterEach } from "vitest";
import { creativeAssets } from "./creativeAssets";
const data = (id: string) => ({
  asset: { id, mime: "image/png", byte_size: 4, sha256: "x" },
  data_base64: btoa("test"),
});
afterEach(() => vi.restoreAllMocks());
it("deduplicates requests, bounds image bytes, and releases URL ownership", async () => {
  const create = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:one"),
    revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  const api = {
      asset: vi.fn().mockImplementation(async (_doc, id) => data(id)),
    },
    assets = creativeAssets(api, "doc", 4);
  const [a, b] = await Promise.all([assets.get("one"), assets.get("one")]);
  expect(a).toBe(b);
  expect(api.asset).toHaveBeenCalledTimes(1);
  await expect(assets.get("two")).rejects.toThrow("preview limit");
  expect(create).toHaveBeenCalledTimes(1);
  assets.dispose();
  expect(revoke).toHaveBeenCalledWith("blob:one");
});
it("does not publish an image after its pane unmounts", async () => {
  const create = vi.spyOn(URL, "createObjectURL");
  let finish!: (value: ReturnType<typeof data>) => void;
  const assets = creativeAssets(
    { asset: () => new Promise((resolve) => (finish = resolve)) },
    "doc",
  );
  const pending = assets.get("one");
  assets.dispose();
  finish(data("one"));
  await expect(pending).rejects.toThrow("Canvas closed");
  expect(create).not.toHaveBeenCalled();
});
