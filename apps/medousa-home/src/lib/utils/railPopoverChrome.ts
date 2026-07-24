/**
 * Lets dock / toolbar search request the rail popover list to open first.
 * Registered by {@link NavRailViewPopover} while a popover is mounted.
 */

export type RailPopoverChrome = {
  /** Expand seed → open (no-op if already open). */
  ensureOpen: () => Promise<void>;
  isOpen: () => boolean;
};

let chrome: RailPopoverChrome | null = null;

export function setRailPopoverChrome(next: RailPopoverChrome | null) {
  chrome = next;
}

export function getRailPopoverChrome(): RailPopoverChrome | null {
  return chrome;
}

/** Expand the active rail popover list, then resolve. */
export async function ensureRailPopoverOpen(): Promise<void> {
  const active = chrome;
  if (!active) return;
  if (active.isOpen()) return;
  await active.ensureOpen();
}
