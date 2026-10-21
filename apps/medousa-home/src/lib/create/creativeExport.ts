import type { CreativeAssets } from "./creativeAssets";
import type { CreativeNode } from "$lib/types/generated/daemon_api";
import { sceneBounds, nodeBounds } from "./creativeScene";
function dataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error("Couldn’t read the canvas image"));
    reader.readAsDataURL(blob);
  });
}
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
  const bounds = frame ? nodeBounds(frame, nodes) : sceneBounds(nodes);
  if (!bounds) throw new Error("Add something to the canvas before exporting");
  if (
    bounds.width > 8192 ||
    bounds.height > 8192 ||
    bounds.width * bounds.height > 32_000_000
  )
    throw new Error(
      "Choose a smaller frame for export (up to 8,192 px per side and 32 megapixels)",
    );
  const clone = svg.cloneNode(true) as SVGSVGElement;
  clone.querySelectorAll("[data-editor-only]").forEach((n) => n.remove());
  const camera = clone.querySelector("[data-camera]");
  camera?.removeAttribute("transform");
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  clone.setAttribute(
    "viewBox",
    `${bounds.x} ${bounds.y} ${bounds.width} ${bounds.height}`,
  );
  clone.setAttribute("width", String(Math.ceil(bounds.width)));
  clone.setAttribute("height", String(Math.ceil(bounds.height)));
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
  for (const image of clone.querySelectorAll<SVGImageElement>(
    "image[data-asset-id]",
  )) {
    const blob = assets.blob(image.dataset.assetId!);
    if (!blob)
      throw new Error("Wait for all canvas images to load before exporting");
    image.setAttribute("href", await dataUrl(blob));
  }
  const source = new Blob([new XMLSerializer().serializeToString(clone)], {
    type: "image/svg+xml",
  });
  let output = source;
  if (format === "png") {
    const url = URL.createObjectURL(source);
    try {
      const image = new Image();
      image.src = url;
      await image.decode();
      const canvas = document.createElement("canvas");
      canvas.width = Math.ceil(bounds.width);
      canvas.height = Math.ceil(bounds.height);
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("Image export is unavailable");
      ctx.drawImage(image, 0, 0);
      output = await new Promise<Blob>((resolve, reject) =>
        canvas.toBlob(
          (b) => (b ? resolve(b) : reject(new Error("Couldn’t encode PNG"))),
          "image/png",
        ),
      );
    } finally {
      URL.revokeObjectURL(url);
    }
  }
  const url = URL.createObjectURL(output),
    link = document.createElement("a");
  link.href = url;
  link.download = `${title.replace(/[^\p{L}\p{N} _-]/gu, "").trim() || "Canvas"}.${format}`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
