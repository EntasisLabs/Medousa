/** Place a dock popover in the viewport (prefer above the trigger). */

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
  const openUp =
    preferUp
      ? spaceAbove >= Math.min(160, maxHeightCap) || spaceAbove >= spaceBelow
      : spaceBelow >= spaceAbove;

  let left = rect.left;
  if (left + width > window.innerWidth - 8) {
    left = window.innerWidth - width - 8;
  }
  if (left < 8) left = 8;

  const maxHeight = Math.min(maxHeightCap, openUp ? spaceAbove : spaceBelow);

  return {
    left,
    top: openUp ? rect.top - gap : rect.bottom + gap,
    transform: openUp ? "translateY(-100%)" : "none",
    maxHeight: Math.max(120, maxHeight),
    width,
  };
}
