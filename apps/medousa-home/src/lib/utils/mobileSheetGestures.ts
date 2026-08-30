import { haptic } from "$lib/haptics";

const DISMISS_THRESHOLD_PX = 64;
const EXPAND_THRESHOLD_PX = 48;
const BACK_THRESHOLD_PX = 56;
const DISMISS_MAX_HORIZONTAL_PX = 48;
const BACK_MAX_VERTICAL_PX = 64;
const DIRECTION_LOCK_PX = 10;
const SHEET_SETTLE_MS = 220;
const EXPANDED_HEIGHT_CSS =
  "calc(var(--mobile-layout-height, 100dvh) - var(--mobile-keyboard-inset, 0px) - max(1rem, env(safe-area-inset-top, 0px)))";

const INTERACTIVE_SELECTOR = [
  "a",
  "button",
  "input",
  "textarea",
  "select",
  "label",
  "[role='button']",
  "[role='option']",
  "[role='menuitem']",
  "[contenteditable='true']",
  ".cm-editor",
  ".cm-scroller",
  ".cm-content",
].join(", ");

export interface MobileSheetGestureOptions {
  onDismiss: () => void;
  /** Return true when a nested screen handled swipe-back. False dismisses the sheet. */
  onSwipeBack?: () => boolean;
  /** When false, horizontal edge-swipe navigation is disabled. Default true. */
  swipeBack?: boolean;
  /** Swipe up on the header to fill the safe viewport. Default true. */
  expandable?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
}

function eventTargetElement(target: EventTarget | null): Element | null {
  if (target instanceof Element) return target;
  if (target instanceof Node) return target.parentElement;
  return null;
}

function shouldIgnoreGestureTarget(target: EventTarget | null): boolean {
  const el = eventTargetElement(target);
  if (!el) return false;
  return Boolean(el.closest(INTERACTIVE_SELECTOR));
}

function prefersReducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function attachVerticalSheetGestures(
  headerEl: HTMLElement,
  sheetEl: HTMLElement,
  options: MobileSheetGestureOptions,
): () => void {
  let startX = 0;
  let startY = 0;
  let startHeight = 0;
  let collapsedHeight = sheetEl.getBoundingClientRect().height;
  let tracking = false;
  let vertical = false;
  let expanded = sheetEl.dataset.sheetExpanded === "true";
  let settleTimer: number | null = null;
  const expandable = options.expandable !== false;

  function clearSettleTimer() {
    if (settleTimer === null) return;
    window.clearTimeout(settleTimer);
    settleTimer = null;
  }

  function setExpandedState(next: boolean) {
    if (expanded === next) return;
    expanded = next;
    sheetEl.classList.toggle("mobile-sheet-expanded", next);
    if (next) sheetEl.dataset.sheetExpanded = "true";
    else delete sheetEl.dataset.sheetExpanded;
    options.onExpandedChange?.(next);
  }

  function resetMotionStyles() {
    sheetEl.style.transform = "";
    sheetEl.style.transition = "";
    if (expanded) {
      sheetEl.style.height = EXPANDED_HEIGHT_CSS;
      sheetEl.style.maxHeight = EXPANDED_HEIGHT_CSS;
    } else {
      sheetEl.style.height = "";
      sheetEl.style.maxHeight = "";
    }
  }

  function settle(nextExpanded: boolean) {
    clearSettleTimer();
    const currentHeight = sheetEl.getBoundingClientRect().height;
    sheetEl.style.transform = "";
    sheetEl.style.transition = "none";
    sheetEl.style.height = `${currentHeight}px`;
    sheetEl.style.maxHeight = `${currentHeight}px`;
    setExpandedState(nextExpanded);

    if (prefersReducedMotion()) {
      resetMotionStyles();
      return;
    }

    // Commit the current frame before animating to the selected resting height.
    void sheetEl.offsetHeight;
    sheetEl.style.transition = `height ${SHEET_SETTLE_MS}ms cubic-bezier(0.22, 1, 0.36, 1), max-height ${SHEET_SETTLE_MS}ms cubic-bezier(0.22, 1, 0.36, 1)`;
    const targetHeight = nextExpanded
      ? EXPANDED_HEIGHT_CSS
      : `${collapsedHeight}px`;
    sheetEl.style.height = targetHeight;
    sheetEl.style.maxHeight = targetHeight;
    settleTimer = window.setTimeout(() => {
      settleTimer = null;
      resetMotionStyles();
    }, SHEET_SETTLE_MS);
  }

  function onTouchStart(event: TouchEvent) {
    if (event.touches.length !== 1) {
      tracking = false;
      vertical = false;
      return;
    }
    if (shouldIgnoreGestureTarget(event.target)) return;
    clearSettleTimer();
    startX = event.touches[0].clientX;
    startY = event.touches[0].clientY;
    startHeight = sheetEl.getBoundingClientRect().height;
    if (!expanded) collapsedHeight = startHeight;
    tracking = true;
    vertical = false;
  }

  function onTouchMove(event: TouchEvent) {
    if (!tracking || event.touches.length !== 1) return;
    const dx = event.touches[0].clientX - startX;
    const dy = event.touches[0].clientY - startY;
    if (!vertical) {
      if (Math.abs(dx) < DIRECTION_LOCK_PX && Math.abs(dy) < DIRECTION_LOCK_PX) return;
      if (Math.abs(dx) > Math.abs(dy)) {
        tracking = false;
        return;
      }
      vertical = true;
    }

    if (dy < 0 && !expanded && expandable) {
      const viewportHeight = window.visualViewport?.height ?? window.innerHeight;
      const fullHeight = Math.max(collapsedHeight, viewportHeight - 16);
      const height = Math.min(fullHeight, startHeight + Math.abs(dy) * 0.9);
      sheetEl.style.transition = "none";
      sheetEl.style.transform = "";
      sheetEl.style.height = `${height}px`;
      sheetEl.style.maxHeight = `${height}px`;
    } else if (dy > 0 && expanded) {
      const height = Math.max(collapsedHeight, startHeight - dy * 0.85);
      sheetEl.style.transition = "none";
      sheetEl.style.transform = "";
      sheetEl.style.height = `${height}px`;
      sheetEl.style.maxHeight = `${height}px`;
    } else if (dy > 0 && !prefersReducedMotion()) {
      sheetEl.style.transition = "none";
      sheetEl.style.transform = `translateY(${Math.min(dy * 0.85, 140)}px)`;
    }
    if (Math.abs(dy) > DIRECTION_LOCK_PX) {
      event.preventDefault();
    }
  }

  function onTouchEnd(event: TouchEvent) {
    if (!tracking) return;
    tracking = false;
    vertical = false;
    const touch = event.changedTouches[0];
    if (!touch) {
      settle(expanded);
      return;
    }
    const dx = touch.clientX - startX;
    const dy = touch.clientY - startY;
    if (Math.abs(dx) > DISMISS_MAX_HORIZONTAL_PX) {
      settle(expanded);
      return;
    }
    if (!expanded && expandable && dy <= -EXPAND_THRESHOLD_PX) {
      settle(true);
      haptic("light");
      return;
    }
    if (expanded && dy >= EXPAND_THRESHOLD_PX) {
      settle(false);
      haptic("light");
      return;
    }
    if (!expanded && dy >= DISMISS_THRESHOLD_PX) {
      resetMotionStyles();
      options.onDismiss();
      return;
    }
    settle(expanded);
  }

  function onTouchCancel() {
    tracking = false;
    vertical = false;
    settle(expanded);
  }

  headerEl.addEventListener("touchstart", onTouchStart, { passive: true });
  headerEl.addEventListener("touchmove", onTouchMove, { passive: false });
  headerEl.addEventListener("touchend", onTouchEnd, { passive: true });
  headerEl.addEventListener("touchcancel", onTouchCancel, { passive: true });

  return () => {
    clearSettleTimer();
    headerEl.removeEventListener("touchstart", onTouchStart);
    headerEl.removeEventListener("touchmove", onTouchMove);
    headerEl.removeEventListener("touchend", onTouchEnd);
    headerEl.removeEventListener("touchcancel", onTouchCancel);
    expanded = false;
    sheetEl.classList.remove("mobile-sheet-expanded");
    delete sheetEl.dataset.sheetExpanded;
    resetMotionStyles();
  };
}

/** Only treat edge swipes as back — avoids killing taps in list sheets. */
const SWIPE_BACK_EDGE_PX = 28;

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
    const touch = event.touches[0];
    const sheetLeft = sheetEl.getBoundingClientRect().left;
    // Full-sheet horizontal tracking races list taps on iOS (preventDefault cancels click).
    if (touch.clientX - sheetLeft > SWIPE_BACK_EDGE_PX) return;
    startX = touch.clientX;
    startY = touch.clientY;
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
    // Only cancel the default once the gesture is clearly a back swipe.
    if (horizontal && dx >= BACK_THRESHOLD_PX) {
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

/** Swipe up/down on the header to expand/collapse/dismiss; edge-swipe right to go back. */
export function attachMobileSheetGestures(
  sheetEl: HTMLElement,
  headerEl: HTMLElement | null,
  options: MobileSheetGestureOptions,
): () => void {
  const cleanups: Array<() => void> = [];
  if (options.swipeBack !== false) {
    cleanups.push(attachSwipeRightNavigation(sheetEl, options));
  }
  if (headerEl) {
    cleanups.push(attachVerticalSheetGestures(headerEl, sheetEl, options));
  }
  return () => {
    for (const cleanup of cleanups) cleanup();
  };
}
