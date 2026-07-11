/** Hide native browser embed while Svelte popovers are open (native layer draws over DOM). */

import { getBrowserCompositor } from "$lib/utils/browserCompositor";
import { isTauri } from "$lib/platform";

let overlayDepth = 0;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;

export function getBrowserPopoverOverlayDepth(): number {
  return overlayDepth;
}

function scheduleCompositorLayout() {
  if (debounceTimer != null) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    debounceTimer = null;
    getBrowserCompositor()?.scheduleLayout();
  }, 16);
}

async function flushCompositorLayout() {
  if (debounceTimer != null) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  const compositor = getBrowserCompositor();
  if (!compositor) return;
  await compositor.flushLayout();
}

export async function pushBrowserPopoverOverlay() {
  if (!isTauri()) return;
  overlayDepth += 1;
  // Hide the native embed immediately — z-index cannot lift DOM above WKWebView.
  await flushCompositorLayout();
}

export async function popBrowserPopoverOverlay() {
  if (!isTauri()) return;
  overlayDepth = Math.max(0, overlayDepth - 1);
  if (overlayDepth === 0) {
    scheduleCompositorLayout();
  } else {
    await flushCompositorLayout();
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
