/**
 * Pointer drag for shell tabs → drop on another pane (`[data-group-id]`),
 * or on a pane edge to split. Clicks (including small jitter) still activate.
 */

import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { MAX_SHELL_PANES, type SplitEdge } from "$lib/types/shellTabs";
import { countLeaves } from "$lib/utils/shellSplitTree";
import { pushBrowserPopoverOverlay, popBrowserPopoverOverlay } from "$lib/utils/browserPopoverOverlay";

/** Real panes only — tab strips also carry `data-group-id` for identity. */
const PANE_SELECTOR = ".shell-pane[data-group-id], .shell-tab-notch-pane[data-group-id]";
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
let ghost: HTMLDivElement | null = null;
let browserOverlay = false;

type DropIntent =
  | { kind: "move"; groupId: string }
  | { kind: "split"; groupId: string; edge: SplitEdge }
  | null;

function paneHostAt(x: number, y: number): HTMLElement | null {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  return el.closest(PANE_SELECTOR) as HTMLElement | null;
}

function edgeAt(host: HTMLElement, x: number, y: number): SplitEdge | null {
  const rect = host.getBoundingClientRect();
  if (rect.width < 8 || rect.height < 8) return null;
  // Full editor panes need generous targets; the mini controller keeps compact ones.
  const mini = host.classList.contains("shell-tab-notch-pane");
  const bandX = mini ? Math.min(40, rect.width * .24) : Math.min(180, rect.width * .25);
  const bandY = mini ? Math.min(40, rect.height * .24) : Math.min(140, rect.height * .25);
  const distances: [SplitEdge, number][] = [
    ["left", (x - rect.left) / bandX], ["right", (rect.right - x) / bandX],
    ["top", (y - rect.top) / bandY], ["bottom", (rect.bottom - y) / bandY],
  ];
  const [edge, distance] = distances.sort((a, b) => a[1] - b[1])[0];
  return distance >= 0 && distance <= 1 ? edge : null;
}

function resolveDrop(x: number, y: number, sourceGroup: string | null): DropIntent {
  const host = paneHostAt(x, y);
  const groupId = host?.dataset.groupId?.trim();
  if (!host || !groupId) return null;

  const canSplit = countLeaves(shellTabs.splitRoot) < MAX_SHELL_PANES;
  const edge = canSplit ? edgeAt(host, x, y) : null;
  if (edge) {
    return { kind: "split", groupId, edge };
  }
  if (groupId !== sourceGroup) {
    return { kind: "move", groupId };
  }
  return null;
}

function releaseCapture(pointerId: number | null) {
  if (captureElement && pointerId != null && captureElement.hasPointerCapture(pointerId)) {
    captureElement.releasePointerCapture(pointerId);
  }
  captureElement = null;
}

function cancel() {
  cleanup();
}

function onCancel(event: PointerEvent) {
  if (event.pointerId === activePointerId) cancel();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  event.stopPropagation();
  cancel();
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
  document.removeEventListener("pointercancel", onCancel);
  document.removeEventListener("keydown", onKeydown, true);
  window.removeEventListener("blur", cancel);
  captureElement?.removeEventListener("lostpointercapture", onCancel);
  releaseCapture(pointerId);
  ghost?.remove();
  ghost = null;
  if (browserOverlay) { browserOverlay = false; void popBrowserPopoverOverlay(); }
  activePointerId = null;
  dragTabId = null;
  sourceGroupId = null;
  dragging = false;
  onDragEnd = null;
  shellTabs.tabDropTargetGroupId = null;
  shellTabs.tabDropSplitEdge = null;
  document.body.classList.remove("shell-tab-dragging");
}

function onMove(event: PointerEvent) {
  if (event.pointerId !== activePointerId || !dragTabId) return;
  const dx = event.clientX - startX;
  const dy = event.clientY - startY;
  if (!dragging && dx * dx + dy * dy >= DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) {
    dragging = true;
    document.body.classList.add("shell-tab-dragging");
    ghost = document.createElement("div");
    ghost.className = "shell-tab-drag-ghost";
    ghost.textContent = shellTabs.tabs.find(tab => tab.id === dragTabId)?.title ?? "Tab";
    ghost.setAttribute("aria-hidden", "true");
    document.body.append(ghost);
    browserOverlay = true;
    void pushBrowserPopoverOverlay();
  }
  if (!dragging) return;
  event.preventDefault();
  if (ghost) {
    ghost.style.left = `${event.clientX + 14}px`;
    ghost.style.top = `${event.clientY + 14}px`;
  }

  const intent = resolveDrop(event.clientX, event.clientY, sourceGroupId);
  if (intent?.kind === "split") {
    shellTabs.tabDropSplitEdge = { groupId: intent.groupId, edge: intent.edge };
    shellTabs.tabDropTargetGroupId = null;
  } else if (intent?.kind === "move") {
    shellTabs.tabDropTargetGroupId = intent.groupId;
    shellTabs.tabDropSplitEdge = null;
  } else {
    shellTabs.tabDropTargetGroupId = null;
    shellTabs.tabDropSplitEdge = null;
  }
}

function onUp(event: PointerEvent) {
  if (event.pointerId !== activePointerId || !dragTabId) {
    return;
  }
  const tabId = dragTabId;
  const from = sourceGroupId;
  const wasDragging = dragging;
  const intent = resolveDrop(event.clientX, event.clientY, from);
  const end = onDragEnd;
  cleanup(event.pointerId);

  if (wasDragging && intent?.kind === "split") {
    const ok = shellTabs.splitGroupWithTab(intent.groupId, tabId, intent.edge);
    end?.(ok);
    return;
  }

  const didMove =
    wasDragging && intent?.kind === "move" && Boolean(from && intent.groupId !== from);
  if (didMove && intent?.kind === "move") {
    shellTabs.moveTab(tabId, intent.groupId);
    shellTabs.focusGroup(intent.groupId);
    end?.(true);
    return;
  }

  // An aborted drag must not switch the active document. A click still selects.
  if (!wasDragging) void shellTabs.activate(tabId);
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
  event.preventDefault(); // Keep native text/image dragging out of this gesture.

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
  document.addEventListener("pointercancel", onCancel);
  document.addEventListener("keydown", onKeydown, true);
  window.addEventListener("blur", cancel);

  if (target) {
    captureElement = target;
    target.addEventListener("lostpointercapture", onCancel);
    try {
      target.setPointerCapture(event.pointerId);
    } catch {
      /* ignore */
    }
  }
}
