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

/** Grow drawer down (toolbar on top) or up (toolbar on bottom). */
export type RailPopoverExpand = "up" | "down";

/** Click / caret point — selection-bubble style anchoring. */
export type RailPopoverCursor = { x: number; y: number };

/** ~16rem — prefer down when this much room exists below the anchor. */
const USEFUL_DRAWER_PX = 16 * 16;
const DEFAULT_OPEN_MAX_PX = 32 * 16;
const BAR_MIN_PX = 48;
/** Gap between cursor and the toolbar strip (matches selection format bubble feel). */
const CURSOR_GAP_PX = 12;

function anchorYFromTriggerOrCursor(
  trigger: HTMLElement,
  cursor?: RailPopoverCursor | null,
): { top: number; bottom: number } {
  if (cursor) {
    return { top: cursor.y, bottom: cursor.y };
  }
  const tr = trigger.getBoundingClientRect();
  return { top: tr.top, bottom: tr.bottom };
}

/**
 * Pick expand direction from cursor/trigger geometry.
 * Prefer down when there is a useful drawer below, otherwise the side with more room.
 */
export function resolveRailPopoverExpand(
  trigger: HTMLElement,
  options?: { pad?: number; prefer?: RailPopoverExpand; cursor?: RailPopoverCursor | null },
): RailPopoverExpand {
  if (options?.prefer) return options.prefer;
  const pad = options?.pad ?? 8;
  const view = viewportBox();
  const anchor = anchorYFromTriggerOrCursor(trigger, options?.cursor);
  // Down: toolbar above/at anchor, body fills toward viewport bottom.
  const spaceDown = Math.max(0, view.top + view.height - pad - anchor.top);
  // Up: toolbar at/below anchor, body fills toward viewport top.
  const spaceUp = Math.max(0, anchor.bottom - (view.top + pad));
  if (spaceDown >= USEFUL_DRAWER_PX || spaceDown >= spaceUp) return "down";
  return "up";
}

/**
 * Max open height that fits on the expand side (scroll inside instead of cropping).
 */
export function railPopoverOpenHeightCap(
  trigger: HTMLElement,
  expand: RailPopoverExpand,
  options?: { pad?: number; maxHeight?: number; cursor?: RailPopoverCursor | null },
): number {
  const pad = options?.pad ?? 8;
  const maxHeight = options?.maxHeight ?? DEFAULT_OPEN_MAX_PX;
  const view = viewportBox();
  const anchor = anchorYFromTriggerOrCursor(trigger, options?.cursor);
  const available =
    expand === "down"
      ? view.top + view.height - pad - anchor.top
      : anchor.bottom - (view.top + pad);
  return Math.max(BAR_MIN_PX, Math.min(maxHeight, Math.floor(available)));
}

/**
 * Place a fixed popover beside a rail trigger, or beside the mouse when `cursor` is set
 * (selection-format-bubble style: centered on the point, above/below with a gap).
 *
 * `expand: "down"` pins the top (drawer grows downward).
 * `expand: "up"` pins the bottom (drawer grows upward via `bottom`).
 * Pass `lockEdge` during height animations to avoid jump-repositioning.
 */
export function placeRailPopover(
  trigger: HTMLElement,
  menu: HTMLElement,
  options?: {
    gap?: number;
    pad?: number;
    alignY?: "center" | "start";
    /** @deprecated Prefer `lockEdge: "top"`. */
    lockTop?: boolean;
    /** Keep the trigger-adjacent edge fixed while height animates. */
    lockEdge?: "top" | "bottom";
    /** Drawer open direction. Defaults from {@link resolveRailPopoverExpand} when omitted with align start. */
    expand?: RailPopoverExpand;
    /** Preferred max height (px) for the open drawer; also written to maxHeight style. */
    openHeight?: number;
    /**
     * When set, float next to the cursor like the live selection toolbar
     * instead of docking to the right of the rail trigger.
     */
    cursor?: RailPopoverCursor | null;
    /** Gap between cursor and toolbar when `cursor` is set. */
    cursorGap?: number;
  },
): void {
  const gap = options?.gap ?? 8;
  const pad = options?.pad ?? 8;
  const cursorGap = options?.cursorGap ?? CURSOR_GAP_PX;
  const alignY = options?.alignY ?? "center";
  const cursor = options?.cursor ?? null;
  const lockEdge =
    options?.lockEdge ?? (options?.lockTop ? "top" : undefined);
  const view = viewportBox();
  const maxW = Math.max(0, view.width - pad * 2);
  const maxH = Math.max(0, view.height - pad * 2);

  const expand =
    options?.expand ??
    (alignY === "start"
      ? resolveRailPopoverExpand(trigger, { pad, cursor })
      : undefined);

  const openHeight =
    options?.openHeight ??
    (expand
      ? railPopoverOpenHeightCap(trigger, expand, {
          pad,
          maxHeight: DEFAULT_OPEN_MAX_PX,
          cursor,
        })
      : maxH);

  // Constrain before measuring so tall/wide menus shrink into the viewport.
  menu.style.maxWidth = `${Math.round(maxW)}px`;
  menu.style.maxHeight = `${Math.round(Math.min(maxH, openHeight))}px`;

  const tr = trigger.getBoundingClientRect();
  const measured = menu.getBoundingClientRect();
  const menuW = Math.min(menu.offsetWidth || measured.width, maxW);
  const menuH = Math.min(menu.offsetHeight || measured.height, Math.min(maxH, openHeight));
  // Toolbar strip height for cursor pinning (seed / chrome), not full open height.
  const barH = Math.min(menuH, BAR_MIN_PX + 8);

  const minLeft = view.left + pad;
  const maxLeft = view.left + view.width - pad - menuW;
  const minTop = view.top + pad;
  const maxTop = view.top + view.height - pad - menuH;

  let left: number;
  if (cursor) {
    // Float just to the right of the click (flip left if that would overflow).
    left = cursor.x + cursorGap;
    if (left > maxLeft) {
      left = cursor.x - cursorGap - menuW;
    }
  } else {
    // Prefer to the right of the trigger; flip left if that would overflow.
    left = tr.right + gap;
    if (left > maxLeft) {
      left = tr.left - gap - menuW;
    }
  }
  left = clamp(left, minLeft, maxLeft);

  if (expand === "up") {
    // Pin bottom — height growth expands upward.
    // With cursor: vertically center the toolbar strip on the click.
    let bottomInset: number;
    if (lockEdge === "bottom") {
      const currentBottom = Number.parseFloat(menu.style.bottom);
      bottomInset = Number.isFinite(currentBottom)
        ? currentBottom
        : view.top + view.height - measured.bottom;
    } else if (cursor) {
      const menuBottom = cursor.y + barH / 2;
      bottomInset = view.top + view.height - menuBottom;
    } else if (lockEdge === "top") {
      const currentTop = Number.parseFloat(menu.style.top);
      if (Number.isFinite(currentTop)) {
        bottomInset = view.top + view.height - (currentTop + menuH);
      } else {
        bottomInset = view.top + view.height - tr.bottom;
      }
    } else {
      bottomInset = view.top + view.height - tr.bottom;
    }
    const maxBottomInset = view.height - pad - BAR_MIN_PX;
    const minBottomInset = pad;
    bottomInset = clamp(bottomInset, minBottomInset, maxBottomInset);

    menu.style.top = "auto";
    menu.style.bottom = `${Math.round(bottomInset)}px`;
    menu.style.left = `${Math.round(left)}px`;
    return;
  }

  let top: number;
  if (lockEdge === "top" || options?.lockTop) {
    const current = Number.parseFloat(menu.style.top);
    top = Number.isFinite(current) ? current : measured.top;
    // Don't re-clamp against the tall menu height — that yanks the card upward mid-collapse.
    const toolbarFloor = view.top + view.height - pad - BAR_MIN_PX;
    top = clamp(top, minTop, toolbarFloor);
  } else if (cursor && (expand === "down" || alignY === "start")) {
    // To the right of the click; vertically center the toolbar on the cursor.
    top = cursor.y - barH / 2;
    top = clamp(top, minTop, maxTop);
  } else if (expand === "down" || alignY === "start") {
    // Grow/shrink downward from the trigger — no vertical jump on height changes.
    top = tr.top;
    top = clamp(top, minTop, maxTop);
  } else {
    // Prefer vertically centered on the trigger, then clamp into the viewport.
    top = tr.top + tr.height / 2 - menuH / 2;
    top = clamp(top, minTop, maxTop);
  }

  menu.style.bottom = "auto";
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
