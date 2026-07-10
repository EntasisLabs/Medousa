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
  /** Consecutive failures that trip the breaker Closed -> Open. */
  failureThreshold: number;
  /** How long the breaker stays Open before allowing a half-open probe. */
  cooldownMs?: number;
  /** Consecutive half-open successes required to fully close again. */
  successThreshold?: number;
}

/** Default cooldown before a tripped breaker allows a half-open probe. */
export const DEFAULT_BREAKER_COOLDOWN_MS = 15_000;

export type CircuitState = "closed" | "open" | "half_open";

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

/**
 * Time-based circuit breaker mirroring the Rust `comms/backoff.rs` state
 * machine (Closed / Open / HalfOpen). A tripped breaker stays Open for
 * `cooldownMs`, then `allow(now)` promotes it to HalfOpen and lets a single
 * probe through — so an open breaker can heal itself instead of starving its
 * own reset (the previous boolean-`open` version could never recover without
 * an app restart, since `allow()` blocked the very attempt that would call
 * `onSuccess()`).
 */
export class CircuitBreaker {
  private state: CircuitState = "closed";
  private consecutiveFailures = 0;
  private consecutiveSuccesses = 0;
  private openedAt: number | null = null;
  private readonly failureThreshold: number;
  private readonly cooldownMs: number;
  private readonly successThreshold: number;

  constructor(config: CircuitBreakerConfig) {
    this.failureThreshold = config.failureThreshold;
    this.cooldownMs = config.cooldownMs ?? DEFAULT_BREAKER_COOLDOWN_MS;
    this.successThreshold = config.successThreshold ?? 1;
  }

  get currentState(): CircuitState {
    return this.state;
  }

  /** Back-compat accessor: true when the breaker is fully tripped. */
  get open(): boolean {
    return this.state === "open";
  }

  /**
   * Whether a request/attempt is permitted at `now`, transitioning
   * Open -> HalfOpen once the cooldown has elapsed.
   */
  allow(now: number = Date.now()): boolean {
    if (this.state === "open") {
      const elapsed = this.openedAt == null ? this.cooldownMs : now - this.openedAt;
      if (elapsed >= this.cooldownMs) {
        this.state = "half_open";
        this.consecutiveSuccesses = 0;
        return true;
      }
      return false;
    }
    return true;
  }

  onSuccess(): void {
    this.consecutiveFailures = 0;
    if (this.state === "half_open") {
      this.consecutiveSuccesses += 1;
      if (this.consecutiveSuccesses >= this.successThreshold) {
        this.state = "closed";
        this.consecutiveSuccesses = 0;
        this.openedAt = null;
      }
    } else if (this.state === "open") {
      this.state = "closed";
      this.openedAt = null;
    }
  }

  onFailure(now: number = Date.now()): void {
    this.consecutiveSuccesses = 0;
    if (this.state === "half_open") {
      this.trip(now);
    } else if (this.state === "closed") {
      this.consecutiveFailures += 1;
      if (this.consecutiveFailures >= this.failureThreshold) {
        this.trip(now);
      }
    }
  }

  /** Milliseconds until an Open breaker will permit a half-open probe. */
  remainingCooldownMs(now: number = Date.now()): number {
    if (this.state !== "open" || this.openedAt == null) return 0;
    return Math.max(0, this.cooldownMs - (now - this.openedAt));
  }

  /** Force back to a clean Closed state (deliberate reconnect / resume). */
  reset(): void {
    this.state = "closed";
    this.consecutiveFailures = 0;
    this.consecutiveSuccesses = 0;
    this.openedAt = null;
  }

  private trip(now: number): void {
    this.state = "open";
    this.openedAt = now;
    this.consecutiveFailures = 0;
    this.consecutiveSuccesses = 0;
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
    // A deliberate cancel (foreground resume, explicit reconnect, teardown) is a
    // clean slate — clear any tripped breaker so recovery isn't locked out.
    this.breaker.reset();
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

    const now = Date.now();
    const wasOpen = this.breaker.currentState === "open";
    if (!this.breaker.allow(now)) {
      // Breaker is Open and cooldown hasn't elapsed. Rather than dropping the
      // reconnect loop entirely (the old behavior — which meant a tripped
      // breaker never recovered without an app restart), arm a wake-up timer
      // for the remaining cooldown so a half-open probe fires automatically.
      this.armBreakerWakeup(task, now);
      return;
    }
    if (wasOpen) {
      // Just transitioned Open -> HalfOpen: this probe is a fresh chance, so
      // restart backoff (and re-arm the interactive attempt budget).
      this.attempt = 0;
    }
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

  private armBreakerWakeup(task: () => void | Promise<void>, now: number): void {
    const wait = Math.max(250, this.breaker.remainingCooldownMs(now));
    this.timer = setTimeout(() => {
      this.timer = null;
      this.schedule(task);
    }, wait);
  }
}
