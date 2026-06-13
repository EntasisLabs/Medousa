import {
  mobileOverlaysOpen,
  stepMobileTab,
  tryMobileBackNavigation,
} from "$lib/mobileNavigation";
import { haptic } from "$lib/haptics";

const SWIPE_THRESHOLD_PX = 56;
const SWIPE_MAX_VERTICAL_PX = 64;
const HORIZONTAL_LOCK_PX = 12;

function shouldIgnoreSwipeTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return true;
  return Boolean(
    target.closest(
      [
        "[data-no-tab-swipe]",
        ".context-map-viewport",
        ".mobile-swipe-row",
        ".mobile-sheet-backdrop",
        ".mobile-story-overlay",
        "input",
        "textarea",
        "select",
        "[contenteditable='true']",
      ].join(", "),
    ),
  );
}

/** Horizontal swipe between bottom tabs; swipe right also walks nested back stacks. */
export function attachMobileTabSwipe(root: HTMLElement): () => void {
  let startX = 0;
  let startY = 0;
  let tracking = false;
  let horizontal = false;

  function reset() {
    tracking = false;
    horizontal = false;
  }

  function onTouchStart(event: TouchEvent) {
    if (event.touches.length !== 1) {
      reset();
      return;
    }
    if (mobileOverlaysOpen()) return;
    if (shouldIgnoreSwipeTarget(event.target)) return;
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
      if (Math.abs(dx) < HORIZONTAL_LOCK_PX && Math.abs(dy) < HORIZONTAL_LOCK_PX) return;
      if (Math.abs(dy) > Math.abs(dx)) {
        reset();
        return;
      }
      horizontal = true;
    }
    if (horizontal && Math.abs(dx) > HORIZONTAL_LOCK_PX) {
      event.preventDefault();
    }
  }

  function onTouchEnd(event: TouchEvent) {
    if (!tracking) return;
    const touch = event.changedTouches[0];
    if (!touch) {
      reset();
      return;
    }
    const dx = touch.clientX - startX;
    const dy = touch.clientY - startY;
    reset();

    if (!horizontal) return;
    if (Math.abs(dy) > SWIPE_MAX_VERTICAL_PX) return;
    if (Math.abs(dx) < SWIPE_THRESHOLD_PX) return;

    if (dx > 0) {
      if (tryMobileBackNavigation()) {
        haptic("light");
        return;
      }
      if (stepMobileTab(-1)) return;
      return;
    }

    stepMobileTab(1);
  }

  root.addEventListener("touchstart", onTouchStart, { passive: true });
  root.addEventListener("touchmove", onTouchMove, { passive: false });
  root.addEventListener("touchend", onTouchEnd, { passive: true });
  root.addEventListener("touchcancel", reset, { passive: true });

  return () => {
    root.removeEventListener("touchstart", onTouchStart);
    root.removeEventListener("touchmove", onTouchMove);
    root.removeEventListener("touchend", onTouchEnd);
    root.removeEventListener("touchcancel", reset);
  };
}
