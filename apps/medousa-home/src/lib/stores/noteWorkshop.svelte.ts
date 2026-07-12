/** Phase G5 — floating note workshop panel state. */

const POSITION_KEY = "medousa-note-workshop-position";

function defaultPosition(): { x: number; y: number } {
  if (typeof window === "undefined") {
    return { x: 24, y: 24 };
  }
  const width = 420;
  const height = 560;
  return {
    x: Math.max(16, window.innerWidth - width - 24),
    y: Math.max(16, window.innerHeight - height - 24),
  };
}

function loadPosition(): { x: number; y: number } {
  if (typeof localStorage === "undefined") return defaultPosition();
  try {
    const raw = JSON.parse(localStorage.getItem(POSITION_KEY) ?? "null") as {
      x?: number;
      y?: number;
    } | null;
    if (typeof raw?.x === "number" && typeof raw?.y === "number") {
      return { x: raw.x, y: raw.y };
    }
  } catch {
    // ignore
  }
  return defaultPosition();
}

function savePosition(x: number, y: number) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(POSITION_KEY, JSON.stringify({ x, y }));
}

export class NoteWorkshopStore {
  open = $state(false);
  minimized = $state(false);
  notePath = $state<string | null>(null);
  x = $state(loadPosition().x);
  y = $state(loadPosition().y);

  openForNote(path: string) {
    this.notePath = path;
    this.open = true;
    this.minimized = false;
    if (this.x <= 0 && this.y <= 0) {
      const pos = defaultPosition();
      this.x = pos.x;
      this.y = pos.y;
    }
  }

  close() {
    this.open = false;
    this.minimized = false;
    this.notePath = null;
  }

  toggleMinimize() {
    this.minimized = !this.minimized;
  }

  setPosition(x: number, y: number, options?: { persist?: boolean }) {
    const margin = 8;
    const width = Math.min(420, typeof window !== "undefined" ? window.innerWidth - margin * 2 : 420);
    const height = this.minimized ? 48 : Math.min(560, typeof window !== "undefined" ? window.innerHeight - margin * 2 : 560);
    const maxX =
      typeof window !== "undefined"
        ? Math.max(margin, window.innerWidth - width - margin)
        : x;
    const maxY =
      typeof window !== "undefined"
        ? Math.max(margin, window.innerHeight - height - margin)
        : y;
    this.x = Math.min(Math.max(margin, x), maxX);
    this.y = Math.min(Math.max(margin, y), maxY);
    if (options?.persist !== false) {
      savePosition(this.x, this.y);
    }
  }

  /** Fit the floating panel into the current viewport (sticky pop-out). */
  fitToViewport(options?: { persist?: boolean }) {
    const margin = 8;
    this.setPosition(margin, margin, options);
  }

  resetPosition() {
    const pos = defaultPosition();
    this.setPosition(pos.x, pos.y);
  }
}

export const noteWorkshop = new NoteWorkshopStore();
