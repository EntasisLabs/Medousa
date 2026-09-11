/**
 * Per-session interaction buffer (events up, awaiting the model).
 *
 * When a user interacts with a rendered scene (selects a chip, expands a card,
 * pins a node), the structured `SceneEvent` lands here rather than firing a new
 * turn. Accepted turn creation acknowledges the bounded envelopes only after
 * the daemon owns them. Plain (non-rune) on purpose — nothing renders from it,
 * and acknowledgement needs predictable, testable semantics.
 */

import type {
  LiquidEventDisposition,
  LiquidInteractionEnvelope,
  SceneEvent,
} from "$lib/liquid/core";

export interface InteractionEntry {
  sessionId: string;
  /** Chat message whose scene emitted the event (turn association for the daemon). */
  messageId: string;
  event: SceneEvent;
}

/** Max retained events per session; oldest are evicted first. */
const MAX_PER_SESSION = 50;
const MAX_PAYLOAD_BYTES = 4 * 1024;

function disposition(event: SceneEvent): LiquidEventDisposition {
  if (event.disposition) return event.disposition;
  if (event.type === "submit" || event.type === "run") return "submit_turn";
  if (event.type === "navigate") return "navigation";
  if (["edit", "pin", "reorder", "dismiss"].includes(event.type)) return "local_state";
  return "context_only";
}

function boundedPayload(payload: unknown): unknown | undefined {
  if (payload === undefined) return undefined;
  try {
    const encoded = JSON.stringify(payload);
    if (encoded.length > MAX_PAYLOAD_BYTES) return undefined;
    return JSON.parse(encoded) as unknown;
  } catch {
    return undefined;
  }
}

export function interactionEnvelope(entry: InteractionEntry): LiquidInteractionEnvelope {
  const payload = boundedPayload(entry.event.payload);
  return {
    version: 1,
    session_id: entry.sessionId.slice(0, 512),
    message_id: entry.messageId.slice(0, 512),
    node_id: entry.event.nodeId.slice(0, 512),
    instance_id: (entry.event.instanceId?.trim() || entry.event.nodeId).slice(0, 512),
    event_type: entry.event.type,
    disposition: disposition(entry.event),
    ...(payload === undefined ? {} : { payload }),
    occurred_at_utc: new Date(entry.event.ts).toISOString(),
    ...(entry.event.expectedStateRevision === undefined
      ? {}
      : { expected_state_revision: Math.max(0, Math.floor(entry.event.expectedStateRevision)) }),
  };
}

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

  /** Drop the oldest acknowledged entries only after turn admission succeeds. */
  ack(sessionId: string, count: number): void {
    if (!Number.isFinite(count) || count <= 0) return;
    const entries = this.bySession.get(sessionId);
    if (!entries) return;
    entries.splice(0, Math.min(entries.length, Math.floor(count)));
    if (entries.length === 0) this.bySession.delete(sessionId);
  }

  envelopes(sessionId: string): LiquidInteractionEnvelope[] {
    return this.peek(sessionId).map(interactionEnvelope);
  }

  /** Clear every session (called on session switch). */
  reset(): void {
    this.bySession.clear();
  }
}

export const chatInteractions = new ChatInteractionBuffer();
