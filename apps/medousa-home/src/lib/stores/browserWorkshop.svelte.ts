/** Floating browser workshop panel state (vault note workshop pattern). */

const POSITION_KEY = "medousa-browser-workshop-position";

function defaultPosition(): { x: number; y: number } {
  if (typeof window === "undefined") {
    return { x: 24, y: 24 };
  }
  const width = 420;
  const height = 560;
  return {
    x: Math.max(16, window.innerWidth - width - 24),
    y: Math.max(16, window.innerHeight - height - 120),
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

export class BrowserWorkshopStore {
  open = $state(false);
  minimized = $state(false);
  scopedSessionId = $state<string | null>(null);
  scopedTabGroupId = $state<string | null>(null);
  scopeLabel = $state("Web");
  x = $state(loadPosition().x);
  y = $state(loadPosition().y);

  openForBrowser(input: {
    sessionId?: string | null;
    tabGroupId?: string | null;
    scopeLabel?: string;
  }) {
    this.scopedSessionId = input.sessionId?.trim() || null;
    this.scopedTabGroupId = input.tabGroupId?.trim() || null;
    this.scopeLabel = input.scopeLabel?.trim() || "Web";
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
    this.scopedSessionId = null;
    this.scopedTabGroupId = null;
    this.scopeLabel = "Web";
  }

  toggleMinimize() {
    this.minimized = !this.minimized;
  }

  setPosition(x: number, y: number) {
    const margin = 8;
    const width = 420;
    const height = this.minimized ? 48 : 560;
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
    savePosition(this.x, this.y);
  }
}

export const browserWorkshop = new BrowserWorkshopStore();
