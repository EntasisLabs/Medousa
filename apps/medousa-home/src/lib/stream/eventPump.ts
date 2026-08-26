import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";

export interface StreamEventTarget {
  sessionId: string;
  event: TurnStreamEnvelopeV3;
}

type ScheduleFlush = (flush: () => void) => () => void;

interface PendingAppend {
  latest: StreamEventTarget;
  type: "content_append" | "reasoning_append";
  segmentId: string | null;
  chunks: string[];
}

const BACKGROUND_FLUSH_MS = 50;
const MAX_TRACKED_STREAMS = 1_024;

function scheduleBrowserFlush(flush: () => void): () => void {
  let finished = false;
  let frameId: number | null = null;
  const finish = () => {
    if (finished) return;
    finished = true;
    if (frameId != null) cancelAnimationFrame(frameId);
    clearTimeout(timerId);
    flush();
  };
  const timerId = setTimeout(finish, BACKGROUND_FLUSH_MS);
  if (typeof requestAnimationFrame === "function") {
    frameId = requestAnimationFrame(finish);
  }
  return () => {
    if (finished) return;
    finished = true;
    if (frameId != null) cancelAnimationFrame(frameId);
    clearTimeout(timerId);
  };
}

/**
 * Coalesces token-only events without delaying semantic stream boundaries.
 *
 * The pump is deliberately non-reactive. Append storms become one store apply
 * per live turn per animation frame, while terminal/tool/approval/reset events
 * synchronously flush prior text before they are applied.
 */
export class StreamEventPump {
  private readonly pendingAppends = new Map<string, PendingAppend>();
  private readonly acceptedSeq = new Map<string, number>();
  private cancelScheduledFlush: (() => void) | null = null;

  constructor(
    private readonly apply: (target: StreamEventTarget) => void,
    private readonly scheduleFlush: ScheduleFlush = scheduleBrowserFlush,
  ) {}

  enqueue(target: StreamEventTarget, appliedSeq: number): void {
    const key = streamKey(target);
    const seq = target.event.seq ?? 0;
    if (seq > 0) {
      const highest = Math.max(appliedSeq, this.acceptedSeq.get(key) ?? 0);
      if (seq <= highest) return;
      this.acceptedSeq.set(key, seq);
      if (this.acceptedSeq.size > MAX_TRACKED_STREAMS) {
        const oldest = this.acceptedSeq.keys().next().value;
        if (oldest != null) this.acceptedSeq.delete(oldest);
      }
    }

    const append = appendEvent(target.event);
    if (!append) {
      this.flushKey(key);
      this.apply(target);
      return;
    }

    const pending = this.pendingAppends.get(key);
    if (
      pending &&
      pending.type === append.type &&
      pending.segmentId === append.segmentId
    ) {
      pending.latest = target;
      pending.chunks.push(append.text);
    } else {
      if (pending) this.flushKey(key);
      this.pendingAppends.set(key, {
        latest: target,
        type: append.type,
        segmentId: append.segmentId,
        chunks: [append.text],
      });
    }
    this.ensureScheduled();
  }

  flush(): void {
    this.cancelScheduledFlush?.();
    this.cancelScheduledFlush = null;
    const pending = [...this.pendingAppends.values()];
    this.pendingAppends.clear();
    for (const append of pending) this.apply(materializeAppend(append));
  }

  /** Drop queued frames when their workshop authority is no longer active. */
  reset(): void {
    this.cancelScheduledFlush?.();
    this.cancelScheduledFlush = null;
    this.pendingAppends.clear();
    this.acceptedSeq.clear();
  }

  private flushKey(key: string): void {
    const pending = this.pendingAppends.get(key);
    if (!pending) return;
    this.pendingAppends.delete(key);
    this.apply(materializeAppend(pending));
    if (this.pendingAppends.size === 0) {
      this.cancelScheduledFlush?.();
      this.cancelScheduledFlush = null;
    }
  }

  private ensureScheduled(): void {
    if (this.cancelScheduledFlush) return;
    this.cancelScheduledFlush = this.scheduleFlush(() => {
      this.cancelScheduledFlush = null;
      this.flush();
    });
  }
}

function streamKey(target: StreamEventTarget): string {
  return `${target.sessionId}\u0000${target.event.turn_id}`;
}

function appendEvent(event: TurnStreamEnvelopeV3):
  | {
      type: "content_append" | "reasoning_append";
      segmentId: string | null;
      text: string;
    }
  | null {
  if (event.event.type === "content_append") {
    return {
      type: event.event.type,
      segmentId: event.event.segment_id,
      text: event.event.text,
    };
  }
  if (event.event.type === "reasoning_append") {
    return { type: event.event.type, segmentId: null, text: event.event.text };
  }
  return null;
}

function materializeAppend(pending: PendingAppend): StreamEventTarget {
  const text = pending.chunks.join("");
  return {
    sessionId: pending.latest.sessionId,
    event: {
      ...pending.latest.event,
      event:
        pending.type === "content_append"
          ? {
              type: "content_append",
              segment_id: pending.segmentId ?? "",
              text,
            }
          : { type: "reasoning_append", text },
    },
  };
}
