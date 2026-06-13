const HIDDEN_ACTIVITY_KEY = "medousa-home-activity-hidden";

export class ActivityViewStore {
  hiddenIds = $state(loadHiddenIds());

  /** Hide feed rows locally — does not delete daemon history. */
  clearViewed(eventIds: string[]) {
    if (eventIds.length === 0) return;
    const next = new Set(this.hiddenIds);
    for (const id of eventIds) {
      if (id.trim()) next.add(id);
    }
    this.hiddenIds = next;
    persistHiddenIds(next);
  }

  restoreAll() {
    this.hiddenIds = new Set();
    persistHiddenIds(new Set());
  }

  /** Drop hidden ids that rolled off the feed tail. */
  pruneToFeed(feedIds: ReadonlySet<string>) {
    if (this.hiddenIds.size === 0) return;
    const next = new Set<string>();
    for (const id of this.hiddenIds) {
      if (feedIds.has(id)) next.add(id);
    }
    if (next.size === this.hiddenIds.size) return;
    this.hiddenIds = next;
    persistHiddenIds(next);
  }
}

function loadHiddenIds(): Set<string> {
  if (typeof localStorage === "undefined") return new Set();
  try {
    const raw = localStorage.getItem(HIDDEN_ACTIVITY_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((id): id is string => typeof id === "string" && id.length > 0));
  } catch {
    return new Set();
  }
}

function persistHiddenIds(ids: ReadonlySet<string>) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(HIDDEN_ACTIVITY_KEY, JSON.stringify([...ids]));
}

export const activityView = new ActivityViewStore();
