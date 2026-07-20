/**
 * Pointer drag for shell tabs → drop on another pane (`[data-group-id]`).
 * Clicks (including with small pointer jitter) still activate the tab.
 */

import { shellTabs } from "$lib/stores/shellTabs.svelte";

const PANE_SELECTOR = "[data-group-id]";
/** Higher than a typical click wobble so left/right tab selects still work. */
const DRAG_THRESHOLD_PX = 10;

let dragTabId: string | null = null;
let sourceGroupId: string | null = null;
let activePointerId: number | null = null;
let startX = 0;
let startY = 0;
let dragging = false;
let moveListener: ((event: PointerEvent) => void) | null = null;
let upListener: ((event: PointerEvent) => void) | null = null;
let captureElement: HTMLElement | null = null;
let onDragEnd: ((didMove: boolean) => void) | null = null;

function groupIdAt(x: number, y: number): string | null {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  const host = el.closest(PANE_SELECTOR) as HTMLElement | null;
  const id = host?.dataset.groupId?.trim();
  return id || null;
}

function releaseCapture(pointerId: number | null) {
  if (captureElement && pointerId != null && captureElement.hasPointerCapture(pointerId)) {
    captureElement.releasePointerCapture(pointerId);
  }
  captureElement = null;
}

function cleanup(pointerId: number | null = activePointerId) {
  if (moveListener) {
    document.removeEventListener("pointermove", moveListener);
    moveListener = null;
  }
  if (upListener) {
    document.removeEventListener("pointerup", upListener);
    upListener = null;
  }
  releaseCapture(pointerId);
  activePointerId = null;
  dragTabId = null;
  sourceGroupId = null;
  dragging = false;
  onDragEnd = null;
  shellTabs.tabDropTargetGroupId = null;
  document.body.classList.remove("shell-tab-dragging");
}

function onMove(event: PointerEvent) {
  if (event.pointerId !== activePointerId || !dragTabId) return;
  const dx = event.clientX - startX;
  const dy = event.clientY - startY;
  if (!dragging && dx * dx + dy * dy >= DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) {
    dragging = true;
    document.body.classList.add("shell-tab-dragging");
  }
  if (!dragging) return;
  const target = groupIdAt(event.clientX, event.clientY);
  shellTabs.tabDropTargetGroupId =
    target && target !== sourceGroupId ? target : null;
}

function onUp(event: PointerEvent) {
  if (event.pointerId !== activePointerId || !dragTabId) {
    cleanup(event.pointerId);
    return;
  }
  const tabId = dragTabId;
  const from = sourceGroupId;
  const wasDragging = dragging;
  const target = groupIdAt(event.clientX, event.clientY);
  const end = onDragEnd;
  cleanup(event.pointerId);

  const didMove =
    wasDragging && Boolean(target && from && target !== from);
  if (didMove && target) {
    shellTabs.moveTab(tabId, target);
    shellTabs.focusGroup(target);
    end?.(true);
    return;
  }

  // Click or aborted drag — always select the pressed tab.
  void shellTabs.activate(tabId);
  end?.(false);
}

/** Begin a potential tab drag from pointerdown on a tab handle. */
export function beginShellTabDrag(
  event: PointerEvent,
  tabId: string,
  groupId: string,
  options?: { onDragEnd?: (didMove: boolean) => void },
) {
  if (event.button !== 0) return;
  if (activePointerId != null) return;

  const target = event.currentTarget as HTMLElement | null;
  dragTabId = tabId;
  sourceGroupId = groupId;
  activePointerId = event.pointerId;
  startX = event.clientX;
  startY = event.clientY;
  dragging = false;
  onDragEnd = options?.onDragEnd ?? null;

  moveListener = onMove;
  upListener = onUp;
  document.addEventListener("pointermove", moveListener);
  document.addEventListener("pointerup", upListener);

  if (target) {
    captureElement = target;
    try {
      target.setPointerCapture(event.pointerId);
    } catch {
      /* ignore */
    }
  }
}
