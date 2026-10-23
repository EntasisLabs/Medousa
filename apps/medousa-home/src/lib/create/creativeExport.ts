import type { CreativeAssets } from "./creativeAssets";
import type { CreativeNode } from "$lib/types/generated/daemon_api";
import { nodeBounds } from "./creativeScene";
import { unionVisualBounds, visualBounds } from "./creativeAppearance";
import { cloneCreativeSvg, creativeSvgBlob, rasterizeCreativeSvg } from "./creativeRender";
/** Serialize the same scene renderer, embedding daemon-owned image bytes. */
export async function exportCreativeScene(
  svg: SVGSVGElement,
  nodes: CreativeNode[],
  assets: CreativeAssets,
  title: string,
  format: "png" | "svg",
  frameId?: string,
) {
  const frame = nodes.find(
    (n) => n.id === frameId && n.content.kind === "frame",
  );
  const bounds = frame ? nodeBounds(frame, nodes) : unionVisualBounds(nodes.filter(n => n.visible && !n.parent_id).map(n => visualBounds(n, nodes)));
  if (!bounds) throw new Error("Add something to the canvas before exporting");
  if (
    bounds.width > 8192 ||
    bounds.height > 8192 ||
    bounds.width * bounds.height > 32_000_000
  )
    throw new Error(
      "Choose a smaller frame for export (up to 8,192 px per side and 32 megapixels)",
    );
  const clone = cloneCreativeSvg(svg, bounds);
  if (frame) {
    const keep = new Set([frame.id]);
    for (const n of nodes)
      if (n.parent_id && keep.has(n.parent_id)) keep.add(n.id);
    // Nodes need not be stored parent-first.
    for (let changed = true; changed;) {
      changed = false;
      for (const n of nodes)
        if (n.parent_id && keep.has(n.parent_id) && !keep.has(n.id)) {
          keep.add(n.id);
          changed = true;
        }
    }
    clone.querySelectorAll<SVGGElement>("[data-node-id]").forEach((n) => {
      if (
        !keep.has(n.dataset.nodeId!) &&
        !n.querySelector(`[data-node-id="${frame.id}"]`)
      )
        n.remove();
    });
  }
  const source = await creativeSvgBlob(clone, assets);
  const output = format === "png" ? await rasterizeCreativeSvg(source, bounds.width, bounds.height) : source;
  const url = URL.createObjectURL(output),
    link = document.createElement("a");
  link.href = url;
  link.download = `${title.replace(/[^\p{L}\p{N} _-]/gu, "").trim() || "Canvas"}.${format}`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
