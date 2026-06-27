/** Hide native browser embed while Svelte popovers are open (native layer draws over DOM). */

import { humanBrowserEmbedHide, humanBrowserEmbedShow } from "$lib/humanBrowser";
import { isTauri } from "$lib/platform";

let overlayDepth = 0;

export async function pushBrowserPopoverOverlay() {
  if (!isTauri()) return;
  overlayDepth += 1;
  if (overlayDepth === 1) {
    await humanBrowserEmbedHide();
  }
}

export async function popBrowserPopoverOverlay() {
  if (!isTauri()) return;
  overlayDepth = Math.max(0, overlayDepth - 1);
  if (overlayDepth === 0) {
    await humanBrowserEmbedShow();
  }
}

export type PopoverPlacement = "above" | "below" | "panel";

export function popoverStyle(
  anchorRect: DOMRect | null | undefined,
  placement: PopoverPlacement,
  options?: { width?: number; maxHeight?: number },
): string {
  const width = options?.width ?? 320;
  const maxHeight = options?.maxHeight ?? 360;

  if (placement === "panel" || !anchorRect) {
    return [
      "left:50%",
      "top:50%",
      `width:min(${width}px,calc(100vw - 2rem))`,
      `max-height:min(${maxHeight}px,calc(100vh - 6rem))`,
      "transform:translate(-50%,-50%)",
    ].join(";");
  }

  const margin = 8;
  const viewportW = typeof window !== "undefined" ? window.innerWidth : width;
  const viewportH = typeof window !== "undefined" ? window.innerHeight : maxHeight;
  let left = anchorRect.left + anchorRect.width / 2 - width / 2;
  left = Math.max(margin, Math.min(left, viewportW - width - margin));

  if (placement === "above") {
    const bottom = viewportH - anchorRect.top + margin;
    return [
      `left:${left}px`,
      `bottom:${bottom}px`,
      `width:${width}px`,
      `max-height:min(${maxHeight}px,${Math.max(120, anchorRect.top - margin * 2)}px)`,
    ].join(";");
  }

  const top = anchorRect.bottom + margin;
  return [
    `left:${left}px`,
    `top:${top}px`,
    `width:${width}px`,
    `max-height:min(${maxHeight}px,${Math.max(120, viewportH - top - margin)}px)`,
  ].join(";");
}
