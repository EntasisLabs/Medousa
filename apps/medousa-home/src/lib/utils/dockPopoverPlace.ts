/** Place a dock popover in the viewport (prefer above the trigger by default). */

export type DockPopoverPlacement = {
  left: number;
  top: number;
  transform: string;
  maxHeight: number;
  width: number;
};

export function placeDockPopover(
  trigger: HTMLElement,
  opts: { preferUp?: boolean; width?: number; maxHeight?: number; gap?: number } = {},
): DockPopoverPlacement {
  const width = opts.width ?? 220;
  const maxHeightCap = opts.maxHeight ?? 320;
  const gap = opts.gap ?? 6;
  const preferUp = opts.preferUp ?? true;

  const rect = trigger.getBoundingClientRect();
  const spaceAbove = Math.max(0, rect.top - 8);
  const spaceBelow = Math.max(0, window.innerHeight - rect.bottom - 8);
  const need = Math.min(160, maxHeightCap);
  // Prefer-up: open above when there is room, or when above has at least as much space.
  // Prefer-down: open below unless below is too short and above has more room.
  const openUp = preferUp
    ? spaceAbove >= need || spaceAbove >= spaceBelow
    : spaceBelow < need && spaceAbove > spaceBelow;

  let left = rect.left;
  if (left + width > window.innerWidth - 8) {
    left = window.innerWidth - width - 8;
  }
  if (left < 8) left = 8;

  const avail = Math.max(0, openUp ? spaceAbove : spaceBelow);
  // Never force a height taller than the open side (that spills off-screen).
  const maxHeight = Math.min(maxHeightCap, avail);

  return {
    left,
    top: openUp ? rect.top - gap : rect.bottom + gap,
    transform: openUp ? "translateY(-100%)" : "none",
    maxHeight,
    width,
  };
}
