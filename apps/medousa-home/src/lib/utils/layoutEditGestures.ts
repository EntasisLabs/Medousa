export const LAYOUT_EDIT_LONG_PRESS_MS = 450;
export const LAYOUT_EDIT_DOUBLE_TAP_MS = 320;

export interface LayoutEditGestureHandlers {
  onSingleTap: (targetId: string | null, kind: "component" | "slot" | null) => void;
  onLongPress: (targetId: string, kind: "component" | "slot") => void;
  onDoubleTap: (targetId: string | null, kind: "component" | "slot" | null) => void;
}

export function createLayoutEditGestureState() {
  let lastTapAt = 0;
  let lastTapId: string | null = null;
  let longPressTimer: ReturnType<typeof setTimeout> | null = null;

  function clearLongPress() {
    if (longPressTimer) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  }

  function handlePointerDown(
    targetId: string,
    kind: "component" | "slot",
    handlers: LayoutEditGestureHandlers,
  ) {
    clearLongPress();
    longPressTimer = setTimeout(() => {
      longPressTimer = null;
      handlers.onLongPress(targetId, kind);
    }, LAYOUT_EDIT_LONG_PRESS_MS);
  }

  function handlePointerUp(
    targetId: string | null,
    kind: "component" | "slot" | null,
    handlers: LayoutEditGestureHandlers,
  ) {
    clearLongPress();
    if (!targetId || !kind) {
      handlers.onSingleTap(null, null);
      return;
    }
    const now = Date.now();
    if (lastTapId === targetId && now - lastTapAt <= LAYOUT_EDIT_DOUBLE_TAP_MS) {
      lastTapId = null;
      lastTapAt = 0;
      handlers.onDoubleTap(targetId, kind);
      return;
    }
    lastTapId = targetId;
    lastTapAt = now;
    handlers.onSingleTap(targetId, kind);
  }

  function dispose() {
    clearLongPress();
  }

  return { handlePointerDown, handlePointerUp, dispose };
}

export function isMobileLayoutEdit(): boolean {
  if (typeof window === "undefined") return false;
  return window.matchMedia("(max-width: 768px)").matches;
}
