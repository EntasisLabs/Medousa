/**
 * Chat stream pool — concurrent live SSE slots keyed by sessionId.
 * Bootstrap sets maxLiveStreams = MAX_SHELL_PANES (4).
 */

export type ChatStreamSlotStatus = "live" | "cached" | "idle";

export type ChatStreamSlot = {
  sessionId: string;
  status: ChatStreamSlotStatus;
  lastEventAt: number;
};

export class ChatStreamPool {
  maxLiveStreams = $state(1);
  slots = $state<Map<string, ChatStreamSlot>>(new Map());

  get liveSessionIds(): string[] {
    return [...this.slots.values()]
      .filter((slot) => slot.status === "live")
      .map((slot) => slot.sessionId);
  }

  get(sessionId: string): ChatStreamSlot | null {
    return this.slots.get(sessionId) ?? null;
  }

  isLive(sessionId: string): boolean {
    return this.slots.get(sessionId)?.status === "live";
  }

  setMaxLive(n: number) {
    const next = Math.max(1, Math.floor(n));
    this.maxLiveStreams = next;
    this.evictOverflow();
  }

  /**
   * Claim a live slot for `sessionId`. Evicts oldest live slots when over max.
   * Returns the sessions that were demoted to cached.
   */
  acquire(sessionId: string): string[] {
    const trimmed = sessionId.trim();
    if (!trimmed) return [];
    const demoted: string[] = [];
    const now = Date.now();
    const existing = this.slots.get(trimmed);
    if (existing?.status === "live") {
      this.slots = new Map(this.slots).set(trimmed, {
        ...existing,
        lastEventAt: now,
      });
      return demoted;
    }

    const map = new Map(this.slots);
    map.set(trimmed, { sessionId: trimmed, status: "live", lastEventAt: now });
    this.slots = map;
    demoted.push(...this.evictOverflow(trimmed));
    if (demoted.length > 0) {
      void import("$lib/stores/chat.svelte").then(({ chat }) => {
        for (const sessionId of demoted) {
          void chat.onSessionDemoted(sessionId);
        }
      });
    }
    return demoted;
  }

  /** Mark session as cached (no longer live). */
  release(sessionId: string) {
    const trimmed = sessionId.trim();
    if (!trimmed) return;
    const existing = this.slots.get(trimmed);
    if (!existing) return;
    const map = new Map(this.slots);
    map.set(trimmed, { ...existing, status: "cached", lastEventAt: Date.now() });
    this.slots = map;
  }

  touch(sessionId: string) {
    const trimmed = sessionId.trim();
    const existing = this.slots.get(trimmed);
    if (!existing) return;
    const map = new Map(this.slots);
    map.set(trimmed, { ...existing, lastEventAt: Date.now() });
    this.slots = map;
  }

  clear() {
    this.slots = new Map();
  }

  private evictOverflow(keepId?: string): string[] {
    const live = [...this.slots.values()].filter((slot) => slot.status === "live");
    if (live.length <= this.maxLiveStreams) return [];

    const demoted: string[] = [];
    const sorted = live.sort((a, b) => a.lastEventAt - b.lastEventAt);
    let overflow = live.length - this.maxLiveStreams;
    const map = new Map(this.slots);
    for (const slot of sorted) {
      if (overflow <= 0) break;
      if (keepId && slot.sessionId === keepId) continue;
      map.set(slot.sessionId, {
        ...slot,
        status: "cached",
        lastEventAt: Date.now(),
      });
      demoted.push(slot.sessionId);
      overflow -= 1;
    }
    // If still over (keepId forced), demote keepId last-resort others already done
    if (overflow > 0) {
      for (const slot of sorted) {
        if (overflow <= 0) break;
        if (map.get(slot.sessionId)?.status !== "live") continue;
        if (keepId && slot.sessionId === keepId && live.length > 1) continue;
        map.set(slot.sessionId, {
          ...slot,
          status: "cached",
          lastEventAt: Date.now(),
        });
        demoted.push(slot.sessionId);
        overflow -= 1;
      }
    }
    this.slots = map;
    return demoted;
  }
}

export const chatStreamPool = new ChatStreamPool();
