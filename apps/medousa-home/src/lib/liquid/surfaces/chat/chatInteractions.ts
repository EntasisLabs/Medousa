/**
 * Per-session interaction buffer (events up, awaiting the model).
 *
 * When a user interacts with a rendered scene (selects a chip, expands a card,
 * pins a node), the structured `SceneEvent` lands here rather than firing a new
 * turn. This is the client half of the bidirectional loop: the PR5 daemon
 * follow-up will `drain` these into the model's context so it sees what the user
 * did. Plain (non-rune) on purpose — nothing renders from it, and `drain` needs
 * predictable, testable semantics.
 */

import type { SceneEvent } from "$lib/liquid/core";

export interface InteractionEntry {
  sessionId: string;
  /** Chat message whose scene emitted the event (turn association for the daemon). */
  messageId: string;
  event: SceneEvent;
}

/** Max retained events per session; oldest are evicted first. */
const MAX_PER_SESSION = 50;

class ChatInteractionBuffer {
  private bySession = new Map<string, InteractionEntry[]>();

  /** Append an event for a session, evicting the oldest past the cap. */
  record(sessionId: string, messageId: string, event: SceneEvent): void {
    if (!sessionId) return;
    const entries = this.bySession.get(sessionId) ?? [];
    entries.push({ sessionId, messageId, event });
    if (entries.length > MAX_PER_SESSION) {
      entries.splice(0, entries.length - MAX_PER_SESSION);
    }
    this.bySession.set(sessionId, entries);
  }

  /** Return and clear a session's buffered events (for the daemon flush). */
  drain(sessionId: string): InteractionEntry[] {
    const entries = this.bySession.get(sessionId);
    if (!entries || entries.length === 0) return [];
    this.bySession.delete(sessionId);
    return entries;
  }

  /** Non-destructive view of the most recent `n` events (default: all). */
  peek(sessionId: string, n?: number): InteractionEntry[] {
    const entries = this.bySession.get(sessionId) ?? [];
    if (typeof n === "number" && n >= 0) {
      return entries.slice(Math.max(0, entries.length - n));
    }
    return entries.slice();
  }

  /** Clear every session (called on session switch). */
  reset(): void {
    this.bySession.clear();
  }
}

export const chatInteractions = new ChatInteractionBuffer();
