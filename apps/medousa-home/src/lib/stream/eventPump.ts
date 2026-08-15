import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

export interface StreamEventTarget {
  sessionId: string;
  event: InteractiveTurnStreamEvent;
}

type ScheduleFlush = (flush: () => void) => () => void;

interface PendingAppend {
  latest: StreamEventTarget;
  content: string[];
  reasoning: string[];
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

    if (!isAppendEvent(target.event)) {
      this.flushKey(key);
      this.apply(target);
      return;
    }

    const pending = this.pendingAppends.get(key);
    if (pending) {
      pending.latest = target;
      if (target.event.content_delta) pending.content.push(target.event.content_delta);
      if (target.event.reasoning_delta) pending.reasoning.push(target.event.reasoning_delta);
    } else {
      this.pendingAppends.set(key, {
        latest: target,
        content: target.event.content_delta ? [target.event.content_delta] : [],
        reasoning: target.event.reasoning_delta ? [target.event.reasoning_delta] : [],
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

function isAppendEvent(event: InteractiveTurnStreamEvent): boolean {
  return (
    !event.terminal &&
    !event.final_text &&
    (event.event_type === "content_delta" || event.event_type === "reasoning_delta")
  );
}

function materializeAppend(pending: PendingAppend): StreamEventTarget {
  const content = pending.content.join("");
  const reasoning = pending.reasoning.join("");
  return {
    sessionId: pending.latest.sessionId,
    event: {
      ...pending.latest.event,
      event_type: content ? "content_delta" : "reasoning_delta",
      content_delta: content || null,
      reasoning_delta: reasoning || null,
    },
  };
}
