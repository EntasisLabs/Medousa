import type { CreativeAssets } from "./creativeAssets";
import type { DrawRect } from "$lib/draw/drawCamera";

export function cloneCreativeSvg(svg: SVGSVGElement, bounds: DrawRect) {
  const clone = svg.cloneNode(true) as SVGSVGElement;
  clone.querySelectorAll("[data-editor-only]").forEach(n => n.remove());
  clone.querySelector("[data-camera]")?.removeAttribute("transform");
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  clone.setAttribute("viewBox", `${bounds.x} ${bounds.y} ${bounds.width} ${bounds.height}`);
  clone.setAttribute("width", String(Math.ceil(bounds.width)));
  clone.setAttribute("height", String(Math.ceil(bounds.height)));
  return clone;
}
export async function creativeSvgBlob(clone: SVGSVGElement, assets: CreativeAssets) {
  for (const image of clone.querySelectorAll<SVGImageElement>("image[data-asset-id]")) {
    const blob = assets.blob(image.dataset.assetId!);
    if (!blob) throw new Error("Wait for canvas images to load before capturing this selection");
    const url = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader(); reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(new Error("Couldn’t read the canvas image")); reader.readAsDataURL(blob);
    });
    image.setAttribute("href", url);
  }
  return new Blob([new XMLSerializer().serializeToString(clone)], { type: "image/svg+xml" });
}
export async function rasterizeCreativeSvg(source: Blob, width: number, height: number): Promise<Blob> {
  const url = URL.createObjectURL(source);
  try {
    const image = new Image(); image.src = url; await image.decode();
    const canvas = document.createElement("canvas"); canvas.width = Math.max(1, Math.ceil(width)); canvas.height = Math.max(1, Math.ceil(height));
    const ctx = canvas.getContext("2d"); if (!ctx) throw new Error("Canvas image capture is unavailable");
    ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
    return await new Promise<Blob>((resolve, reject) => canvas.toBlob(b => b ? resolve(b) : reject(new Error("Couldn’t encode the canvas image")), "image/png"));
  } finally { URL.revokeObjectURL(url); }
}
