/**
 * Client-side reconnect discipline for Tauri SSE streams.
 * Mirrors medousa-sdk Rust/Python reconnect helpers: bounded backoff,
 * overlap guard, circuit breaker, and spine replay via `?since=<seq>`.
 */

export const MAX_STREAM_RECONNECT_DELAY_MS = 30_000;

export interface BackoffPolicy {
  baseMs: number;
  factor: number;
  maxMs: number;
  maxAttempts: number | null;
}

export interface CircuitBreakerConfig {
  failureThreshold: number;
}

export const DEFAULT_INTERACTIVE_BACKOFF: BackoffPolicy = {
  baseMs: 500,
  factor: 2,
  maxMs: MAX_STREAM_RECONNECT_DELAY_MS,
  maxAttempts: 10,
};

export const DEFAULT_WORKSPACE_BACKOFF: BackoffPolicy = {
  baseMs: 1_000,
  factor: 2,
  maxMs: MAX_STREAM_RECONNECT_DELAY_MS,
  maxAttempts: null,
};

export function reconnectDelayMs(policy: BackoffPolicy, attempt: number): number {
  const raw = Math.min(policy.baseMs * policy.factor ** attempt, policy.maxMs);
  const jitter = 0.5 + ((attempt * 7919) % 500) / 1000;
  return Math.round(raw * jitter);
}

export function mayRetry(policy: BackoffPolicy, attempt: number): boolean {
  if (policy.maxAttempts == null) return true;
  return attempt < policy.maxAttempts;
}

/** Append or replace `?since=` for durable spine replay. */
export function streamPathWithSince(path: string, since: number): string {
  const base = path.split("?")[0] ?? path;
  if (since <= 0) return base;
  return `${base}?since=${since}`;
}

export interface SeqTrackedEvent {
  turn_id: string;
  seq?: number;
}

/** Returns false when the event is a duplicate (seq <= last seen). */
export function applyStreamSeq(
  lastSeqByTurn: Map<string, number>,
  event: SeqTrackedEvent,
): boolean {
  const seq = event.seq ?? 0;
  if (seq <= 0) return true;
  const last = lastSeqByTurn.get(event.turn_id) ?? 0;
  if (seq <= last) return false;
  lastSeqByTurn.set(event.turn_id, seq);
  return true;
}

export class OverlapGuard {
  private active = false;

  tryEnter(): boolean {
    if (this.active) return false;
    this.active = true;
    return true;
  }

  release(): void {
    this.active = false;
  }

  get isActive(): boolean {
    return this.active;
  }
}

export class CircuitBreaker {
  consecutiveFailures = 0;
  open = false;

  constructor(private readonly config: CircuitBreakerConfig) {}

  allow(): boolean {
    return !this.open;
  }

  onSuccess(): void {
    this.consecutiveFailures = 0;
    this.open = false;
  }

  onFailure(): void {
    this.consecutiveFailures += 1;
    if (this.consecutiveFailures >= this.config.failureThreshold) {
      this.open = true;
    }
  }
}

export type ReconnectSchedulerOptions = {
  policy: BackoffPolicy;
  failureThreshold?: number;
  onExhausted?: () => void;
};

/**
 * Schedules bounded reconnect attempts with overlap protection.
 * Used by workshop workspace + interactive SSE recovery loops.
 */
export class ReconnectScheduler {
  private attempt = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readonly overlap = new OverlapGuard();
  private readonly breaker: CircuitBreaker;
  private readonly policy: BackoffPolicy;
  private readonly onExhausted?: () => void;
  private tornDown = false;

  constructor(options: ReconnectSchedulerOptions) {
    this.policy = options.policy;
    this.breaker = new CircuitBreaker({
      failureThreshold: options.failureThreshold ?? 5,
    });
    this.onExhausted = options.onExhausted;
  }

  get pending(): boolean {
    return this.timer != null;
  }

  cancel(): void {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    this.attempt = 0;
    this.overlap.release();
  }

  teardown(): void {
    this.tornDown = true;
    this.cancel();
  }

  /** Call after a successful reconnect to reset backoff. */
  noteSuccess(): void {
    this.attempt = 0;
    this.breaker.onSuccess();
  }

  /** Schedule `task` unless a timer is already pending or overlap rejects. */
  schedule(task: () => void | Promise<void>): void {
    if (this.tornDown || this.timer) return;
    if (!this.breaker.allow()) return;
    if (!mayRetry(this.policy, this.attempt)) {
      this.onExhausted?.();
      return;
    }
    if (!this.overlap.tryEnter()) return;

    const delayMs = reconnectDelayMs(this.policy, this.attempt);
    this.attempt += 1;

    this.timer = setTimeout(() => {
      this.timer = null;
      void Promise.resolve(task())
        .catch(() => {
          this.breaker.onFailure();
        })
        .finally(() => {
          this.overlap.release();
        });
    }, delayMs);
  }
}
