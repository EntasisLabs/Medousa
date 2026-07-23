/**
 * Detect a horizontal mouse-shake gesture (frustrated “where is it?” flick).
 * Used to summon the current view toolbar at the cursor.
 */

import { summonViewToolbar } from "$lib/utils/railPopoverSummon";

export const MOUSE_SHAKE_PREF_KEY = "medousa-mouse-shake-toolbar";

const WINDOW_MS = 450;
const MIN_AMPLITUDE_PX = 28;
const MIN_REVERSALS = 4;
const COOLDOWN_MS = 1200;

export type MouseShakeSample = { x: number; y: number; t: number };

export type MouseShakeDetectorOptions = {
  windowMs?: number;
  minAmplitudePx?: number;
  minReversals?: number;
  cooldownMs?: number;
  now?: () => number;
};

/**
 * Pure detector — feed pointer samples; returns true when a shake fires.
 * Call {@link MouseShakeDetector.reset} after handling if you need a clean slate
 * (cooldown is tracked internally).
 */
export class MouseShakeDetector {
  private samples: MouseShakeSample[] = [];
  /** Negative = never fired (so startup isn’t stuck in cooldown). */
  private lastFireAt = Number.NEGATIVE_INFINITY;
  private readonly windowMs: number;
  private readonly minAmplitudePx: number;
  private readonly minReversals: number;
  private readonly cooldownMs: number;
  private readonly now: () => number;

  constructor(options: MouseShakeDetectorOptions = {}) {
    this.windowMs = options.windowMs ?? WINDOW_MS;
    this.minAmplitudePx = options.minAmplitudePx ?? MIN_AMPLITUDE_PX;
    this.minReversals = options.minReversals ?? MIN_REVERSALS;
    this.cooldownMs = options.cooldownMs ?? COOLDOWN_MS;
    this.now = options.now ?? (() => Date.now());
  }

  reset() {
    this.samples = [];
  }

  /** Clear history and cooldown (tests). */
  resetAll() {
    this.samples = [];
    this.lastFireAt = Number.NEGATIVE_INFINITY;
  }

  push(x: number, y: number, t = this.now()): boolean {
    const now = t;
    if (Number.isFinite(this.lastFireAt) && now - this.lastFireAt < this.cooldownMs) {
      this.samples = [];
      return false;
    }

    this.samples.push({ x, y, t: now });
    const cutoff = now - this.windowMs;
    while (this.samples.length > 0 && (this.samples[0]?.t ?? 0) < cutoff) {
      this.samples.shift();
    }

    if (this.samples.length < 3) return false;

    let reversals = 0;
    let lastSign = 0;
    let travel = 0;

    for (let i = 1; i < this.samples.length; i++) {
      const prev = this.samples[i - 1]!;
      const cur = this.samples[i]!;
      const dx = cur.x - prev.x;
      if (Math.abs(dx) < 2) continue;
      const sign = dx > 0 ? 1 : -1;
      travel += Math.abs(dx);
      if (lastSign !== 0 && sign !== lastSign) {
        reversals += 1;
      }
      lastSign = sign;
    }

    if (reversals >= this.minReversals && travel >= this.minAmplitudePx * this.minReversals) {
      this.lastFireAt = now;
      this.samples = [];
      return true;
    }
    return false;
  }
}

export function isMouseShakeToolbarEnabled(): boolean {
  if (typeof localStorage === "undefined") return true;
  try {
    const raw = localStorage.getItem(MOUSE_SHAKE_PREF_KEY);
    if (raw === null) return true;
    return raw !== "0" && raw !== "false";
  } catch {
    return true;
  }
}

export function setMouseShakeToolbarEnabled(enabled: boolean) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(MOUSE_SHAKE_PREF_KEY, enabled ? "1" : "0");
  } catch {
    // ignore quota / private mode
  }
}

export function toggleMouseShakeToolbarEnabled(): boolean {
  const next = !isMouseShakeToolbarEnabled();
  setMouseShakeToolbarEnabled(next);
  return next;
}

function modalBlocksShake(): boolean {
  if (typeof document === "undefined") return false;
  return Boolean(
    document.querySelector(
      '[role="dialog"][aria-modal="true"], [data-spotlight-open="true"]',
    ),
  );
}

function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || !window.matchMedia) return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export type AttachMouseShakeHandlers = {
  onShake?: () => void;
};

/**
 * Attach window listeners for mouse-shake → summon view toolbar.
 */
export function attachMouseShakeToolbar(
  handlers: AttachMouseShakeHandlers = {},
): () => void {
  const detector = new MouseShakeDetector();
  let buttonsDown = 0;

  const onPointerDown = (event: PointerEvent) => {
    if (event.button >= 0) buttonsDown += 1;
  };
  const onPointerUp = () => {
    buttonsDown = Math.max(0, buttonsDown - 1);
  };
  const onPointerCancel = () => {
    buttonsDown = 0;
    detector.reset();
  };

  const onPointerMove = (event: PointerEvent) => {
    if (!isMouseShakeToolbarEnabled()) return;
    if (prefersReducedMotion()) return;
    if (modalBlocksShake()) {
      detector.reset();
      return;
    }
    if (buttonsDown > 0 || event.buttons !== 0) {
      detector.reset();
      return;
    }

    if (detector.push(event.clientX, event.clientY, performance.now())) {
      if (handlers.onShake) {
        handlers.onShake();
      } else {
        summonViewToolbar({ x: event.clientX, y: event.clientY });
      }
    }
  };

  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("pointerup", onPointerUp, true);
  window.addEventListener("pointercancel", onPointerCancel, true);
  window.addEventListener("pointermove", onPointerMove, { passive: true });

  return () => {
    window.removeEventListener("pointerdown", onPointerDown, true);
    window.removeEventListener("pointerup", onPointerUp, true);
    window.removeEventListener("pointercancel", onPointerCancel, true);
    window.removeEventListener("pointermove", onPointerMove);
  };
}
