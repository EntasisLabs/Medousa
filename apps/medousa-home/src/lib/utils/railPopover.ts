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
 *
 * `alignY: "start"` pins the top to the trigger (drawer grows/shrinks downward).
 * Pass `lockTop` during height animations to avoid jump-repositioning.
 */
export function placeRailPopover(
  trigger: HTMLElement,
  menu: HTMLElement,
  options?: {
    gap?: number;
    pad?: number;
    alignY?: "center" | "start";
    /** Keep the current `style.top` (only refresh left / max bounds). */
    lockTop?: boolean;
  },
): void {
  const gap = options?.gap ?? 8;
  const pad = options?.pad ?? 8;
  const alignY = options?.alignY ?? "center";
  const lockTop = options?.lockTop ?? false;
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

  let top: number;
  if (lockTop) {
    const current = Number.parseFloat(menu.style.top);
    top = Number.isFinite(current) ? current : measured.top;
    // Don't re-clamp against the tall menu height — that yanks the card upward mid-collapse.
    const toolbarFloor = view.top + view.height - pad - 48;
    top = clamp(top, minTop, toolbarFloor);
  } else if (alignY === "start") {
    // Grow/shrink downward from the trigger — no vertical jump on height changes.
    top = tr.top;
    top = clamp(top, minTop, maxTop);
  } else {
    // Prefer vertically centered on the trigger, then clamp into the viewport.
    top = tr.top + tr.height / 2 - menuH / 2;
    top = clamp(top, minTop, maxTop);
  }

  menu.style.top = `${Math.round(top)}px`;
  menu.style.left = `${Math.round(left)}px`;
}

/**
 * Place a fixed popover above a composer trigger (Cursor-style).
 * Prefers above + start-aligned; flips below / clamps into the viewport.
 */
export function placeComposerPopover(
  trigger: HTMLElement,
  menu: HTMLElement,
  options?: { gap?: number; pad?: number },
): void {
  const gap = options?.gap ?? 8;
  const pad = options?.pad ?? 8;
  const view = viewportBox();
  const maxW = Math.max(0, view.width - pad * 2);
  const maxH = Math.max(0, Math.min(view.height - pad * 2, view.height * 0.5));

  menu.style.maxWidth = `${Math.round(Math.min(maxW, 20 * 16))}px`;
  menu.style.maxHeight = `${Math.round(maxH)}px`;

  const tr = trigger.getBoundingClientRect();
  const measured = menu.getBoundingClientRect();
  const menuW = Math.min(menu.offsetWidth || measured.width, maxW, 20 * 16);
  const menuH = Math.min(menu.offsetHeight || measured.height, maxH);

  const minLeft = view.left + pad;
  const maxLeft = view.left + view.width - pad - menuW;
  const minTop = view.top + pad;
  const maxTop = view.top + view.height - pad - menuH;

  let left = tr.left;
  left = clamp(left, minLeft, maxLeft);

  // Prefer above the trigger.
  let top = tr.top - gap - menuH;
  if (top < minTop) {
    top = tr.bottom + gap;
  }
  top = clamp(top, minTop, maxTop);

  menu.style.position = "fixed";
  menu.style.top = `${Math.round(top)}px`;
  menu.style.left = `${Math.round(left)}px`;
}

/**
 * Place a fixed toolbar/dock popover.
 * End-aligns to the trigger; prefers below or above, flips when short on space,
 * and caps max-height to the open side so tall menus scroll instead of cropping.
 */
export function placeToolbarPopover(
  trigger: HTMLElement,
  menu: HTMLElement,
  options?: {
    gap?: number;
    pad?: number;
    /** Preferred width in px before viewport clamp. */
    width?: number;
    /** Dock triggers usually prefer above; titlebars prefer below. */
    prefer?: "below" | "above";
    maxHeightRatio?: number;
  },
): void {
  const gap = options?.gap ?? 6;
  const pad = options?.pad ?? 12;
  const prefer = options?.prefer ?? "below";
  const preferredWidth = options?.width ?? 22 * 16;
  const maxHeightRatio = options?.maxHeightRatio ?? 0.82;
  const view = viewportBox();

  const maxW = Math.max(0, Math.min(preferredWidth, view.width - pad * 2));
  const viewMaxH = Math.max(0, Math.min(view.height - pad * 2, view.height * maxHeightRatio));

  menu.style.position = "fixed";
  menu.style.width = `${Math.round(maxW)}px`;
  menu.style.maxWidth = `${Math.round(maxW)}px`;
  menu.style.maxHeight = `${Math.round(viewMaxH)}px`;

  const tr = trigger.getBoundingClientRect();
  const measured = menu.getBoundingClientRect();
  const menuW = Math.min(menu.offsetWidth || measured.width || maxW, maxW);
  let menuH = Math.min(menu.offsetHeight || measured.height || 0, viewMaxH);

  const minLeft = view.left + pad;
  const maxLeft = view.left + view.width - pad - menuW;
  let left = tr.right - menuW;
  left = clamp(left, minLeft, maxLeft);

  const spaceBelow = view.top + view.height - pad - (tr.bottom + gap);
  const spaceAbove = tr.top - gap - (view.top + pad);
  const need = Math.min(menuH || 160, viewMaxH);

  let openAbove: boolean;
  if (prefer === "above") {
    openAbove = spaceAbove >= need || spaceAbove >= spaceBelow;
  } else {
    openAbove = spaceBelow < need && spaceAbove > spaceBelow;
  }

  const avail = Math.max(0, openAbove ? spaceAbove : spaceBelow);
  const cappedH = Math.max(120, Math.min(viewMaxH, avail || viewMaxH));
  menu.style.maxHeight = `${Math.round(cappedH)}px`;

  // Prefer natural content height (scrollHeight) so growth re-anchors above docks.
  const naturalH = Math.max(
    menu.scrollHeight || 0,
    menu.offsetHeight || 0,
    menu.getBoundingClientRect().height || 0,
  );
  menuH = Math.min(naturalH || cappedH, cappedH);

  const minTop = view.top + pad;
  const maxTop = view.top + view.height - pad - menuH;
  let top = openAbove ? tr.top - gap - menuH : tr.bottom + gap;
  top = clamp(top, minTop, maxTop);

  menu.style.top = `${Math.round(top)}px`;
  menu.style.left = `${Math.round(left)}px`;
  menu.style.overflow = "hidden";
}
