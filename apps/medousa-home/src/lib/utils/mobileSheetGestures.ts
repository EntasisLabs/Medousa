import { haptic } from "$lib/haptics";

const DISMISS_THRESHOLD_PX = 64;
const BACK_THRESHOLD_PX = 56;
const DISMISS_MAX_HORIZONTAL_PX = 48;
const BACK_MAX_VERTICAL_PX = 64;
const DIRECTION_LOCK_PX = 10;

const INTERACTIVE_SELECTOR = [
  "input",
  "textarea",
  "select",
  "[contenteditable='true']",
  ".cm-editor",
  ".cm-scroller",
  ".cm-content",
].join(", ");

export interface MobileSheetGestureOptions {
  onDismiss: () => void;
  /** Return true when a nested screen handled swipe-back. False dismisses the sheet. */
  onSwipeBack?: () => boolean;
}

function shouldIgnoreGestureTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(target.closest(INTERACTIVE_SELECTOR));
}

function prefersReducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function attachSwipeDownDismiss(
  headerEl: HTMLElement,
  sheetEl: HTMLElement,
  onDismiss: () => void,
): () => void {
  let startX = 0;
  let startY = 0;
  let tracking = false;

  function resetTransform() {
    sheetEl.style.transform = "";
    sheetEl.style.transition = "";
  }

  function onTouchStart(event: TouchEvent) {
    if (event.touches.length !== 1) {
      tracking = false;
      return;
    }
    if (shouldIgnoreGestureTarget(event.target)) return;
    startX = event.touches[0].clientX;
    startY = event.touches[0].clientY;
    tracking = true;
  }

  function onTouchMove(event: TouchEvent) {
    if (!tracking || event.touches.length !== 1) return;
    const dx = event.touches[0].clientX - startX;
    const dy = event.touches[0].clientY - startY;
    if (dy <= 0 || Math.abs(dx) > Math.abs(dy)) {
      if (dy <= 0) resetTransform();
      return;
    }
    if (!prefersReducedMotion()) {
      sheetEl.style.transition = "none";
      sheetEl.style.transform = `translateY(${Math.min(dy * 0.85, 140)}px)`;
    }
    if (dy > DIRECTION_LOCK_PX) {
      event.preventDefault();
    }
  }

  function onTouchEnd(event: TouchEvent) {
    if (!tracking) return;
    tracking = false;
    const touch = event.changedTouches[0];
    resetTransform();
    if (!touch) return;
    const dx = touch.clientX - startX;
    const dy = touch.clientY - startY;
    if (dy < DISMISS_THRESHOLD_PX) return;
    if (Math.abs(dx) > DISMISS_MAX_HORIZONTAL_PX) return;
    onDismiss();
  }

  function onTouchCancel() {
    tracking = false;
    resetTransform();
  }

  headerEl.addEventListener("touchstart", onTouchStart, { passive: true });
  headerEl.addEventListener("touchmove", onTouchMove, { passive: false });
  headerEl.addEventListener("touchend", onTouchEnd, { passive: true });
  headerEl.addEventListener("touchcancel", onTouchCancel, { passive: true });

  return () => {
    headerEl.removeEventListener("touchstart", onTouchStart);
    headerEl.removeEventListener("touchmove", onTouchMove);
    headerEl.removeEventListener("touchend", onTouchEnd);
    headerEl.removeEventListener("touchcancel", onTouchCancel);
  };
}

function attachSwipeRightNavigation(
  sheetEl: HTMLElement,
  options: MobileSheetGestureOptions,
): () => void {
  let startX = 0;
  let startY = 0;
  let tracking = false;
  let horizontal = false;

  function onTouchStart(event: TouchEvent) {
    if (event.touches.length !== 1) {
      tracking = false;
      horizontal = false;
      return;
    }
    if (shouldIgnoreGestureTarget(event.target)) return;
    startX = event.touches[0].clientX;
    startY = event.touches[0].clientY;
    tracking = true;
    horizontal = false;
  }

  function onTouchMove(event: TouchEvent) {
    if (!tracking || event.touches.length !== 1) return;
    const dx = event.touches[0].clientX - startX;
    const dy = event.touches[0].clientY - startY;
    if (!horizontal) {
      if (Math.abs(dx) < DIRECTION_LOCK_PX && Math.abs(dy) < DIRECTION_LOCK_PX) return;
      if (Math.abs(dy) > Math.abs(dx) || dx <= 0) {
        tracking = false;
        return;
      }
      horizontal = true;
    }
    if (horizontal && dx > DIRECTION_LOCK_PX) {
      event.preventDefault();
    }
  }

  function onTouchEnd(event: TouchEvent) {
    if (!tracking) return;
    const touch = event.changedTouches[0];
    tracking = false;
    horizontal = false;
    if (!touch) return;
    const dx = touch.clientX - startX;
    const dy = touch.clientY - startY;
    if (Math.abs(dy) > BACK_MAX_VERTICAL_PX) return;
    if (dx < BACK_THRESHOLD_PX) return;
    if (options.onSwipeBack?.()) {
      haptic("light");
      return;
    }
    options.onDismiss();
  }

  function onTouchCancel() {
    tracking = false;
    horizontal = false;
  }

  sheetEl.addEventListener("touchstart", onTouchStart, { passive: true });
  sheetEl.addEventListener("touchmove", onTouchMove, { passive: false });
  sheetEl.addEventListener("touchend", onTouchEnd, { passive: true });
  sheetEl.addEventListener("touchcancel", onTouchCancel, { passive: true });

  return () => {
    sheetEl.removeEventListener("touchstart", onTouchStart);
    sheetEl.removeEventListener("touchmove", onTouchMove);
    sheetEl.removeEventListener("touchend", onTouchEnd);
    sheetEl.removeEventListener("touchcancel", onTouchCancel);
  };
}

/** Swipe down on the sheet header to dismiss; swipe right on the sheet to go back or dismiss. */
export function attachMobileSheetGestures(
  sheetEl: HTMLElement,
  headerEl: HTMLElement | null,
  options: MobileSheetGestureOptions,
): () => void {
  const cleanups = [attachSwipeRightNavigation(sheetEl, options)];
  if (headerEl) {
    cleanups.push(attachSwipeDownDismiss(headerEl, sheetEl, options.onDismiss));
  }
  return () => {
    for (const cleanup of cleanups) cleanup();
  };
}
