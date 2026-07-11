export interface KanbanDragSource {
  columnIndex: number;
  cardIndex: number;
}

export interface KanbanDropTarget {
  columnIndex: number;
  /** Insert index within the column (append when undefined). */
  cardIndex?: number;
  /** When hovering a card: insert before that card vs after. */
  insertBefore?: boolean;
}

const COLUMN_SELECTOR = "[data-kanban-drop-column]";
const CARD_SELECTOR = "[data-kanban-drop-card]";

let dragSource: KanbanDragSource | null = null;
let activePointerId: number | null = null;
let moveListener: ((event: PointerEvent) => void) | null = null;
let upListener: ((event: PointerEvent) => void) | null = null;
let cancelListener: ((event: PointerEvent) => void) | null = null;
let captureElement: HTMLElement | null = null;

function isSameAsSource(target: KanbanDropTarget | null): boolean {
  if (!target || !dragSource || target.cardIndex == null) return false;
  return (
    target.columnIndex === dragSource.columnIndex &&
    target.cardIndex === dragSource.cardIndex
  );
}

export function dropTargetAt(x: number, y: number): KanbanDropTarget | null {
  const target = document.elementFromPoint(x, y);
  if (!target) return null;

  const cardEl = target.closest(CARD_SELECTOR) as HTMLElement | null;
  if (cardEl?.dataset.columnIndex != null && cardEl.dataset.cardIndex != null) {
    const columnIndex = Number(cardEl.dataset.columnIndex);
    const cardIndex = Number(cardEl.dataset.cardIndex);
    if (Number.isNaN(columnIndex) || Number.isNaN(cardIndex)) return null;
    const rect = cardEl.getBoundingClientRect();
    const insertBefore = y < rect.top + rect.height / 2;
    const resolved: KanbanDropTarget = { columnIndex, cardIndex, insertBefore };
    if (isSameAsSource(resolved)) return null;
    return resolved;
  }

  const columnEl = target.closest(COLUMN_SELECTOR) as HTMLElement | null;
  if (columnEl?.dataset.kanbanDropColumn != null) {
    const columnIndex = Number(columnEl.dataset.kanbanDropColumn);
    if (Number.isNaN(columnIndex)) return null;
    return { columnIndex };
  }

  return null;
}

/** Resolve drop target to a concrete insert index for moveCard. */
export function resolveDropInsertIndex(to: KanbanDropTarget): number | undefined {
  if (to.cardIndex == null) return undefined;
  if (to.insertBefore === false) return to.cardIndex + 1;
  return to.cardIndex;
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
  dragSource = null;
  activePointerId = null;
  document.body.classList.remove("vault-kanban-dragging");
}

export function isKanbanPointerDragging(): boolean {
  return dragSource != null;
}

export function currentKanbanDragSource(): KanbanDragSource | null {
  return dragSource;
}

export function startKanbanPointerDrag(
  source: KanbanDragSource,
  onHighlight: (target: KanbanDropTarget | null) => void,
  onComplete: (from: KanbanDragSource, to: KanbanDropTarget) => void,
  event: PointerEvent,
) {
  cleanupPointerDrag();
  dragSource = source;
  activePointerId = event.pointerId;
  document.body.classList.add("vault-kanban-dragging");
  // Do not highlight the source card as a drop target on grab.
  onHighlight(null);

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
    onHighlight(dropTargetAt(moveEvent.clientX, moveEvent.clientY));
  };

  upListener = (upEvent: PointerEvent) => {
    if (activePointerId !== upEvent.pointerId) return;
    const from = dragSource;
    const to = dropTargetAt(upEvent.clientX, upEvent.clientY);
    cleanupPointerDrag();
    if (!from || !to) {
      onHighlight(null);
      return;
    }
    onComplete(from, to);
    onHighlight(null);
  };

  cancelListener = (cancelEvent: PointerEvent) => {
    if (activePointerId !== cancelEvent.pointerId) return;
    cleanupPointerDrag(cancelEvent.pointerId);
    onHighlight(null);
  };

  document.addEventListener("pointermove", moveListener);
  document.addEventListener("pointerup", upListener);
  document.addEventListener("pointercancel", cancelListener);
}

export function cancelKanbanPointerDrag(onHighlight?: (target: KanbanDropTarget | null) => void) {
  cleanupPointerDrag();
  onHighlight?.(null);
}
