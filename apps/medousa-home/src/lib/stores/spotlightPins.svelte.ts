import { activeWorkshopId } from "$lib/utils/workshopLocality";

export type SpotlightPinKind = "note" | "chat" | "script" | "surface";

export interface SpotlightPin {
  kind: SpotlightPinKind;
  /** Note path, session id, script id, or surface name. */
  target: string;
  label: string;
  meta?: {
    scrollTop?: number;
    heading?: string;
  };
}

const MAX_SLOTS = 4;
const STORAGE_PREFIX = "medousa-home-spotlight-pins:";
const LAST_SCRIPT_KEY = "medousa-home-spotlight-last-script";

function workshopKey(): string {
  return activeWorkshopId();
}

function storageKey(): string {
  return `${STORAGE_PREFIX}${workshopKey()}`;
}

function readSlots(): Array<SpotlightPin | null> {
  if (typeof localStorage === "undefined") {
    return Array.from({ length: MAX_SLOTS }, () => null);
  }
  try {
    const raw = localStorage.getItem(storageKey());
    if (!raw) return Array.from({ length: MAX_SLOTS }, () => null);
    const parsed = JSON.parse(raw) as Array<SpotlightPin | null>;
    if (!Array.isArray(parsed)) {
      return Array.from({ length: MAX_SLOTS }, () => null);
    }
    const slots: Array<SpotlightPin | null> = Array.from(
      { length: MAX_SLOTS },
      () => null,
    );
    for (let i = 0; i < MAX_SLOTS; i += 1) {
      const entry = parsed[i];
      if (
        entry &&
        typeof entry === "object" &&
        typeof entry.target === "string" &&
        typeof entry.label === "string" &&
        (entry.kind === "note" ||
          entry.kind === "chat" ||
          entry.kind === "script" ||
          entry.kind === "surface")
      ) {
        slots[i] = entry;
      }
    }
    return slots;
  } catch {
    return Array.from({ length: MAX_SLOTS }, () => null);
  }
}

function writeSlots(slots: Array<SpotlightPin | null>) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(storageKey(), JSON.stringify(slots));
  } catch {
    // ignore quota
  }
}

class SpotlightPinsStore {
  slots = $state<Array<SpotlightPin | null>>(readSlots());
  lastScriptId = $state<string | null>(readLastScriptId());
  /** Workshop id the in-memory slots belong to — avoids reloading on every collect. */
  #boundWorkshopId = workshopKey();

  /**
   * Re-load pins when the active workshop changes.
   * Must not write `slots` when already bound — collect tracks `slots` and would loop.
   */
  ensureWorkshopSynced() {
    const key = workshopKey();
    if (key === this.#boundWorkshopId) return;
    this.#boundWorkshopId = key;
    this.slots = readSlots();
  }

  /** @deprecated use ensureWorkshopSynced */
  syncWorkshop() {
    this.ensureWorkshopSynced();
  }

  pinAt(slotIndex: number, pin: SpotlightPin) {
    if (slotIndex < 0 || slotIndex >= MAX_SLOTS) return;
    const next = [...this.slots];
    next[slotIndex] = pin;
    this.slots = next;
    writeSlots(next);
  }

  /** First empty slot, or replace slot 0 if full. */
  pin(pin: SpotlightPin): number {
    const empty = this.slots.findIndex((slot) => slot == null);
    const index = empty >= 0 ? empty : 0;
    this.pinAt(index, pin);
    return index;
  }

  unpin(slotIndex: number) {
    if (slotIndex < 0 || slotIndex >= MAX_SLOTS) return;
    const next = [...this.slots];
    next[slotIndex] = null;
    this.slots = next;
    writeSlots(next);
  }

  clear() {
    const next = Array.from({ length: MAX_SLOTS }, () => null);
    this.slots = next;
    writeSlots(next);
  }

  get(slotIndex: number): SpotlightPin | null {
    return this.slots[slotIndex] ?? null;
  }

  setLastScriptId(scriptId: string | null) {
    this.lastScriptId = scriptId;
    if (typeof localStorage === "undefined") return;
    try {
      if (scriptId) localStorage.setItem(LAST_SCRIPT_KEY, scriptId);
      else localStorage.removeItem(LAST_SCRIPT_KEY);
    } catch {
      // ignore
    }
  }
}

function readLastScriptId(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return localStorage.getItem(LAST_SCRIPT_KEY);
  } catch {
    return null;
  }
}

export const spotlightPins = new SpotlightPinsStore();
export const SPOTLIGHT_PIN_SLOTS = MAX_SLOTS;
