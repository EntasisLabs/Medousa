import { visualBounds } from "./creativeAppearance";
import { cloneCreativeSvg, creativeSvgBlob, rasterizeCreativeSvg } from "./creativeRender";
import type { CreativeNode } from "$lib/types/generated/daemon_api";
import type { CreativeAssets } from "./creativeAssets";

export function selectionCaptureBounds(nodes: CreativeNode[], selected: string[]) {
  const bounds = nodes.filter(n => selected.includes(n.id) && n.visible).map(n => visualBounds(n, nodes));
  if (!bounds.length) throw new Error("Select visible objects to give Medousa visual context");
  const x = Math.min(...bounds.map(b => b.x)), y = Math.min(...bounds.map(b => b.y));
  const right = Math.max(...bounds.map(b => b.x + b.width)), bottom = Math.max(...bounds.map(b => b.y + b.height));
  const padding = Math.max(12, Math.min(48, Math.max(right - x, bottom - y) * .04));
  return { x: x - padding, y: y - padding, width: Math.max(1, right - x + 2 * padding), height: Math.max(1, bottom - y + 2 * padding) };
}
/** Crop the live renderer, including surrounding artwork inside the crop. Never camera/UI chrome. */
export async function captureCreativeSelection(svg: SVGSVGElement, nodes: CreativeNode[], selected: string[], assets: CreativeAssets) {
  const bounds = selectionCaptureBounds(nodes, selected);
  const clone = cloneCreativeSvg(svg, bounds);
  let scale = Math.min(1, 1536 / Math.max(bounds.width, bounds.height));
  clone.setAttribute("width", String(Math.max(1, Math.ceil(bounds.width * scale))));
  clone.setAttribute("height", String(Math.max(1, Math.ceil(bounds.height * scale))));
  const source = await creativeSvgBlob(clone, assets);
  let image = await rasterizeCreativeSvg(source, bounds.width * scale, bounds.height * scale);
  while (image.size > 4 * 1024 * 1024) {
    scale *= .75;
    image = await rasterizeCreativeSvg(source, bounds.width * scale, bounds.height * scale);
  }
  const bytes = new Uint8Array(await image.arrayBuffer()); let binary = "";
  for (let i = 0; i < bytes.length; i += 32768) binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
  return btoa(binary);
}
