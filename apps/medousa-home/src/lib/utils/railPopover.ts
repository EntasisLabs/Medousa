function clamp(value: number, min: number, max: number): number {
  if (max < min) return min;
  return Math.min(max, Math.max(min, value));
}

function viewportBox(): { left: number; top: number; width: number; height: number } {
  const vv = window.visualViewport;
  if (vv) {
    return {
      left: vv.offsetLeft,
      top: vv.offsetTop,
      width: vv.width,
      height: vv.height,
    };
  }
  return {
    left: 0,
    top: 0,
    width: window.innerWidth,
    height: window.innerHeight,
  };
}

/**
 * Place a fixed popover beside a rail trigger.
 * Caps the menu to the viewport and clamps left/top so it never paints out of bounds.
 */
export function placeRailPopover(
  trigger: HTMLElement,
  menu: HTMLElement,
  options?: { gap?: number; pad?: number },
): void {
  const gap = options?.gap ?? 8;
  const pad = options?.pad ?? 8;
  const view = viewportBox();
  const maxW = Math.max(0, view.width - pad * 2);
  const maxH = Math.max(0, view.height - pad * 2);

  // Constrain before measuring so tall/wide menus shrink into the viewport.
  menu.style.maxWidth = `${Math.round(maxW)}px`;
  menu.style.maxHeight = `${Math.round(maxH)}px`;

  const tr = trigger.getBoundingClientRect();
  const measured = menu.getBoundingClientRect();
  const menuW = Math.min(menu.offsetWidth || measured.width, maxW);
  const menuH = Math.min(menu.offsetHeight || measured.height, maxH);

  const minLeft = view.left + pad;
  const maxLeft = view.left + view.width - pad - menuW;
  const minTop = view.top + pad;
  const maxTop = view.top + view.height - pad - menuH;

  // Prefer to the right of the trigger; flip left if that would overflow.
  let left = tr.right + gap;
  if (left > maxLeft) {
    left = tr.left - gap - menuW;
  }
  left = clamp(left, minLeft, maxLeft);

  // Prefer vertically centered on the trigger, then clamp into the viewport.
  let top = tr.top + tr.height / 2 - menuH / 2;
  top = clamp(top, minTop, maxTop);

  menu.style.top = `${Math.round(top)}px`;
  menu.style.left = `${Math.round(left)}px`;
}
