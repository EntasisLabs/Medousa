/** @vitest-environment happy-dom */
import { it, expect, vi, beforeEach } from "vitest";
import { captureCreativeSelection, selectionCaptureBounds } from "./creativeCapture";
import { createNode } from "./creativeScene";
import { cloneCreativeSvg } from "./creativeRender";
import type { CreativeAssets } from "./creativeAssets";
const { raster, source } = vi.hoisted(() => ({ raster: vi.fn(), source: vi.fn() }));
vi.mock("./creativeRender", async importOriginal => ({ ...await importOriginal<typeof import("./creativeRender")>(), creativeSvgBlob: source, rasterizeCreativeSvg: raster }));
beforeEach(() => { raster.mockReset().mockResolvedValue(new Blob(["png"])); source.mockReset().mockResolvedValue(new Blob(["svg"])); });
it("captures rotated selection bounds with context padding independent of the camera", () => {
  const node = createNode({ kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 1 }, { x: 100, y: 200, width: 200, height: 100 });
  node.transform.rotation = 90;
  expect(selectionCaptureBounds([node], [node.id])).toEqual({ x: 138, y: 138, width: 124, height: 224 });
});
it("reuses the scene renderer while removing selection chrome, camera transforms and huge intrinsic dimensions", async () => {
  const node = createNode({ kind: "frame", name: "Large", background: "#fff" }, { x: 0, y: 0, width: 1e8, height: 1e8 });
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.innerHTML = '<g data-camera transform="translate(200 100) scale(2)"><rect width="100" height="100"/><g data-editor-only><circle/></g></g>';
  const result = await captureCreativeSelection(svg, [node], [node.id], {} as CreativeAssets);
  expect(result).toBe(btoa("png"));
  const clone = source.mock.calls[0][0] as SVGSVGElement;
  expect(clone.querySelector("[data-editor-only]")).toBeNull();
  expect(clone.querySelector("[data-camera]")?.hasAttribute("transform")).toBe(false);
  expect(Number(clone.getAttribute("width"))).toBeLessThanOrEqual(1536);
  expect(Number(clone.getAttribute("height"))).toBeLessThanOrEqual(1536);
  expect(svg.querySelector("[data-editor-only]")).not.toBeNull();
});
it("does not remove artwork while excluding editor-only controls from export/capture", () => {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.innerHTML = '<g data-camera transform="scale(3)"><image data-asset-id="art"/><g data-editor-only><text>90°</text></g></g>';
  const clone = cloneCreativeSvg(svg, { x: 1, y: 2, width: 3, height: 4 });
  expect(clone.querySelector("image")?.getAttribute("data-asset-id")).toBe("art");
  expect(clone.querySelector("text")).toBeNull(); expect(clone.getAttribute("viewBox")).toBe("1 2 3 4");
});
