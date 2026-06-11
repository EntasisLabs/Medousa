export const VAULT_NOTE_DRAG = "application/x-medousa-vault-note";

const DROP_TARGET_SELECTOR = "[data-vault-drop-prefix]";
const DROP_ACTIVE_CLASS = "vault-tree-row--drop-active";
const DRAGGING_BODY_CLASS = "vault-tree-dragging";

let draggingPath: string | null = null;
let activePointerId: number | null = null;
let suppressTreeClickUntil = 0;
let moveListener: ((event: PointerEvent) => void) | null = null;
let upListener: ((event: PointerEvent) => void) | null = null;
let cancelListener: ((event: PointerEvent) => void) | null = null;
let captureElement: HTMLElement | null = null;

function dropPrefixAt(x: number, y: number): string | null {
  const element = document.elementFromPoint(x, y)?.closest(DROP_TARGET_SELECTOR) as
    | HTMLElement
    | null;
  const prefix = element?.dataset.vaultDropPrefix?.trim();
  return prefix ? prefix : null;
}

function highlightDropTarget(prefix: string | null) {
  for (const element of document.querySelectorAll<HTMLElement>(DROP_TARGET_SELECTOR)) {
    element.classList.toggle(
      DROP_ACTIVE_CLASS,
      prefix != null && element.dataset.vaultDropPrefix === prefix,
    );
  }
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
  highlightDropTarget(null);
  draggingPath = null;
  activePointerId = null;
  document.body.classList.remove(DRAGGING_BODY_CLASS);
}

function completeDrag(
  x: number,
  y: number,
  onComplete: (sourcePath: string, targetFolderPrefix: string) => void,
) {
  const source = draggingPath;
  const prefix = dropPrefixAt(x, y);
  cleanupPointerDrag();
  if (!source || !prefix) return;

  const fileName = source.split("/").pop();
  if (!fileName) return;
  const nextPath = `${prefix}${fileName}`.replace(/\/+/g, "/");
  if (nextPath === source) return;

  suppressTreeClickUntil = Date.now() + 250;
  onComplete(source, prefix);
}

export function isVaultPointerDragging(): boolean {
  return draggingPath != null;
}

export function shouldSuppressVaultTreeClick(): boolean {
  return Date.now() < suppressTreeClickUntil;
}

export function startVaultPointerDrag(
  sourcePath: string,
  onComplete: (sourcePath: string, targetFolderPrefix: string) => void,
  event: PointerEvent,
) {
  cleanupPointerDrag();
  draggingPath = sourcePath;
  activePointerId = event.pointerId;
  document.body.classList.add(DRAGGING_BODY_CLASS);

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
    highlightDropTarget(dropPrefixAt(moveEvent.clientX, moveEvent.clientY));
  };

  upListener = (upEvent: PointerEvent) => {
    if (activePointerId !== upEvent.pointerId) return;
    completeDrag(upEvent.clientX, upEvent.clientY, onComplete);
  };

  cancelListener = (cancelEvent: PointerEvent) => {
    if (activePointerId !== cancelEvent.pointerId) return;
    cleanupPointerDrag(cancelEvent.pointerId);
  };

  document.addEventListener("pointermove", moveListener);
  document.addEventListener("pointerup", upListener);
  document.addEventListener("pointercancel", cancelListener);
}

export function cancelVaultPointerDrag() {
  cleanupPointerDrag();
}
