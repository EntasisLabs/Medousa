const PANE_SELECTOR = "[data-layout-pane-id]";
const DRAGGING_BODY_CLASS = "layout-edit-pointer-dragging";

let dragComponentId: string | null = null;
let activePointerId: number | null = null;
let moveListener: ((event: PointerEvent) => void) | null = null;
let upListener: ((event: PointerEvent) => void) | null = null;
let cancelListener: ((event: PointerEvent) => void) | null = null;
let captureElement: HTMLElement | null = null;

export function layoutPaneAt(x: number, y: number): string | null {
  const target = document.elementFromPoint(x, y);
  if (!target) return null;
  const paneEl = target.closest(PANE_SELECTOR) as HTMLElement | null;
  const paneId = paneEl?.dataset.layoutPaneId?.trim();
  return paneId || null;
}

function releaseCapture(pointerId: number | null) {
  if (captureElement && pointerId != null && captureElement.hasPointerCapture(pointerId)) {
    captureElement.releasePointerCapture(pointerId);
  }
  captureElement = null;
}

function cleanupPointerDrag(pointerId: number | null = activePointerId) {
  if (moveListener) {
    document.removeEventListener("pointermove", moveListener);
    moveListener = null;
  }
  if (upListener) {
    document.removeEventListener("pointerup", upListener);
    upListener = null;
  }
  if (cancelListener) {
    document.removeEventListener("pointercancel", cancelListener);
    cancelListener = null;
  }
  releaseCapture(pointerId);
  dragComponentId = null;
  activePointerId = null;
  document.body.classList.remove(DRAGGING_BODY_CLASS);
}

export function isLayoutPointerDragging(): boolean {
  return dragComponentId != null;
}

export function currentLayoutDragComponentId(): string | null {
  return dragComponentId;
}

export function startLayoutPointerDrag(
  componentId: string,
  handlers: {
    onHighlight: (paneId: string | null) => void;
    onComplete: (componentId: string, targetPaneId: string) => void;
    onCancel: () => void;
  },
  event: PointerEvent,
) {
  cleanupPointerDrag();
  dragComponentId = componentId;
  activePointerId = event.pointerId;
  document.body.classList.add(DRAGGING_BODY_CLASS);
  handlers.onHighlight(layoutPaneAt(event.clientX, event.clientY));

  captureElement = event.currentTarget as HTMLElement | null;
  if (captureElement?.setPointerCapture) {
    try {
      captureElement.setPointerCapture(event.pointerId);
    } catch {
      captureElement = null;
    }
  }

  moveListener = (moveEvent: PointerEvent) => {
    if (activePointerId !== moveEvent.pointerId) return;
    handlers.onHighlight(layoutPaneAt(moveEvent.clientX, moveEvent.clientY));
  };

  upListener = (upEvent: PointerEvent) => {
    if (activePointerId !== upEvent.pointerId) return;
    const movingId = dragComponentId;
    const targetPaneId = layoutPaneAt(upEvent.clientX, upEvent.clientY);
    cleanupPointerDrag();
    if (!movingId || !targetPaneId) {
      handlers.onHighlight(null);
      handlers.onCancel();
      return;
    }
    handlers.onComplete(movingId, targetPaneId);
    handlers.onHighlight(null);
  };

  cancelListener = (cancelEvent: PointerEvent) => {
    if (activePointerId !== cancelEvent.pointerId) return;
    cleanupPointerDrag(cancelEvent.pointerId);
    handlers.onHighlight(null);
    handlers.onCancel();
  };

  document.addEventListener("pointermove", moveListener);
  document.addEventListener("pointerup", upListener);
  document.addEventListener("pointercancel", cancelListener);
}

export function cancelLayoutPointerDrag(onHighlight?: (paneId: string | null) => void) {
  cleanupPointerDrag();
  onHighlight?.(null);
}
