import { haptic } from "$lib/haptics";

const DISMISS_THRESHOLD_PX = 64;
const EXPAND_THRESHOLD_PX = 48;
const FLICK_THRESHOLD_PX = 18;
const FLICK_VELOCITY_PX_PER_MS = 0.22;
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
  let startTime = 0;
  let lastY = 0;
  let lastTime = 0;
  let verticalVelocity = 0;
  let minDy = 0;
  let maxDy = 0;
  let collapsedHeight = sheetEl.getBoundingClientRect().height;
  let expandedHeight = collapsedHeight;
  let tracking = false;
  let vertical = false;
  let resizing = false;
  let expanded = sheetEl.dataset.sheetExpanded === "true";
  let settleTimer: number | null = null;
  const expandable = options.expandable !== false;
  const previousTouchAction = headerEl.style.touchAction;
  const previousAnimation = sheetEl.style.animation;

  // Claim vertical drags before the browser turns them into page scrolling and
  // sends touchcancel. Taps on controls still work because they are ignored by
  // the gesture recognizer below.
  headerEl.style.touchAction = "none";

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
    resizing = false;
  }

  function settle(nextExpanded: boolean) {
    clearSettleTimer();
    setExpandedState(nextExpanded);

    if (prefersReducedMotion()) {
      resetMotionStyles();
      return;
    }

    sheetEl.style.transition = "none";
    if (resizing) {
      const collapsedOffset = Math.max(0, expandedHeight - collapsedHeight);
      // The sheet stays full-height while settling; only its composited
      // transform moves. This avoids relaying out a long session list on every
      // animation frame.
      void sheetEl.offsetHeight;
      sheetEl.style.transition = `transform ${SHEET_SETTLE_MS}ms cubic-bezier(0.22, 1, 0.36, 1)`;
      sheetEl.style.transform = nextExpanded
        ? "translate3d(0, 0, 0)"
        : `translate3d(0, ${collapsedOffset}px, 0)`;
    } else {
      // Dismiss/rubber-band drags begin from the collapsed resting size.
      void sheetEl.offsetHeight;
      sheetEl.style.transition = `transform ${SHEET_SETTLE_MS}ms cubic-bezier(0.22, 1, 0.36, 1)`;
      sheetEl.style.transform = "translate3d(0, 0, 0)";
    }

    settleTimer = window.setTimeout(() => {
      settleTimer = null;
      resetMotionStyles();
    }, SHEET_SETTLE_MS);
  }

  function prepareResize() {
    if (resizing) return;
    sheetEl.style.animation = "none";
    sheetEl.style.transition = "none";
    sheetEl.style.transform = "";
    sheetEl.style.height = EXPANDED_HEIGHT_CSS;
    sheetEl.style.maxHeight = EXPANDED_HEIGHT_CSS;
    expandedHeight = Math.max(
      collapsedHeight,
      sheetEl.getBoundingClientRect().height,
    );
    const collapsedOffset = Math.max(0, expandedHeight - collapsedHeight);
    sheetEl.style.transform = expanded
      ? "translate3d(0, 0, 0)"
      : `translate3d(0, ${collapsedOffset}px, 0)`;
    resizing = true;
  }

  function updateMotionSample(event: TouchEvent, y: number) {
    const now = event.timeStamp;
    const dt = now - lastTime;
    if (dt > 0 && dt <= 120) {
      verticalVelocity = (y - lastY) / dt;
    }
    lastY = y;
    lastTime = now;
    const dy = y - startY;
    minDy = Math.min(minDy, dy);
    maxDy = Math.max(maxDy, dy);
  }

  function onTouchStart(event: TouchEvent) {
    if (event.touches.length !== 1) {
      tracking = false;
      vertical = false;
      return;
    }
    if (shouldIgnoreGestureTarget(event.target)) return;
    if (settleTimer !== null) {
      clearSettleTimer();
      resetMotionStyles();
    }
    sheetEl.style.animation = "none";
    startX = event.touches[0].clientX;
    startY = event.touches[0].clientY;
    startTime = event.timeStamp;
    lastY = startY;
    lastTime = startTime;
    verticalVelocity = 0;
    minDy = 0;
    maxDy = 0;
    if (!expanded) collapsedHeight = sheetEl.getBoundingClientRect().height;
    tracking = true;
    vertical = false;
  }

  function onTouchMove(event: TouchEvent) {
    if (!tracking || event.touches.length !== 1) return;
    const dx = event.touches[0].clientX - startX;
    const dy = event.touches[0].clientY - startY;
    updateMotionSample(event, event.touches[0].clientY);
    if (!vertical) {
      if (Math.abs(dx) < DIRECTION_LOCK_PX && Math.abs(dy) < DIRECTION_LOCK_PX) return;
      if (Math.abs(dx) > Math.abs(dy)) {
        tracking = false;
        return;
      }
      vertical = true;
    }

    if (!expanded && expandable && (resizing || dy < 0)) {
      prepareResize();
      const collapsedOffset = Math.max(0, expandedHeight - collapsedHeight);
      const dragFactor = dy < 0 ? 0.9 : 0.85;
      const offset = Math.max(
        0,
        Math.min(collapsedOffset + 140, collapsedOffset + dy * dragFactor),
      );
      sheetEl.style.transition = "none";
      sheetEl.style.transform = `translate3d(0, ${offset}px, 0)`;
    } else if (expanded && expandable && (resizing || dy > 0)) {
      prepareResize();
      const collapsedOffset = Math.max(0, expandedHeight - collapsedHeight);
      const offset = Math.max(0, Math.min(collapsedOffset, dy * 0.85));
      sheetEl.style.transition = "none";
      sheetEl.style.transform = `translate3d(0, ${offset}px, 0)`;
    } else if (dy > 0 && !prefersReducedMotion()) {
      sheetEl.style.transition = "none";
      sheetEl.style.transform = `translate3d(0, ${Math.min(dy * 0.85, 140)}px, 0)`;
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
    updateMotionSample(event, touch.clientY);
    if (Math.abs(dx) > DISMISS_MAX_HORIZONTAL_PX) {
      settle(expanded);
      return;
    }
    const elapsed = event.timeStamp - startTime;
    const overallVelocity = elapsed > 0 ? dy / elapsed : 0;
    const recentVelocity = event.timeStamp - lastTime <= 80 ? verticalVelocity : 0;
    const upwardFlick =
      minDy <= -FLICK_THRESHOLD_PX &&
      elapsed >= 8 &&
      Math.min(overallVelocity, recentVelocity) <= -FLICK_VELOCITY_PX_PER_MS;
    const downwardFlick =
      maxDy >= FLICK_THRESHOLD_PX &&
      elapsed >= 8 &&
      Math.max(overallVelocity, recentVelocity) >= FLICK_VELOCITY_PX_PER_MS;
    if (
      !expanded &&
      expandable &&
      (minDy <= -EXPAND_THRESHOLD_PX || upwardFlick)
    ) {
      settle(true);
      haptic("light");
      return;
    }
    if (expanded && (maxDy >= EXPAND_THRESHOLD_PX || downwardFlick)) {
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
    if (!expanded && expandable && minDy <= -EXPAND_THRESHOLD_PX) {
      settle(true);
      haptic("light");
      return;
    }
    if (expanded && maxDy >= EXPAND_THRESHOLD_PX) {
      settle(false);
      haptic("light");
      return;
    }
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
    headerEl.style.touchAction = previousTouchAction;
    sheetEl.style.animation = previousAnimation;
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
