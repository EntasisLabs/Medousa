/**
 * Summon the current view’s compact rail toolbar at the cursor.
 * Registered by NavShell; called from hotkeys / mouse-shake.
 */

export type RailPopoverCursor = { x: number; y: number };

export type RailPopoverSummonHandler = (cursor?: RailPopoverCursor | null) => boolean;

let handler: RailPopoverSummonHandler | null = null;

/** Last known pointer — used when hotkey fires without a fresh mouse event. */
let lastPointer: RailPopoverCursor | null = null;

export function registerRailPopoverSummon(next: RailPopoverSummonHandler | null) {
  handler = next;
}

export function getLastPointer(): RailPopoverCursor | null {
  return lastPointer;
}

export function setLastPointer(cursor: RailPopoverCursor) {
  lastPointer = cursor;
}

/**
 * Open (or toggle-close) the current surface toolbar at the cursor.
 * @returns true if a handler handled the request.
 */
export function summonViewToolbar(cursor?: RailPopoverCursor | null): boolean {
  const active = handler;
  if (!active) return false;
  return active(cursor ?? lastPointer);
}
