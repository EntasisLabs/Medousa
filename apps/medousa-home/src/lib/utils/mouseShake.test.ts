import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  MouseShakeDetector,
  isMouseShakeToolbarEnabled,
  setMouseShakeToolbarEnabled,
  MOUSE_SHAKE_PREF_KEY,
} from "./mouseShake";

describe("MouseShakeDetector", () => {
  it("fires after enough horizontal reversals with amplitude", () => {
    let t = 1000;
    const det = new MouseShakeDetector({
      now: () => t,
      windowMs: 500,
      minAmplitudePx: 20,
      minReversals: 4,
      cooldownMs: 1000,
    });

    // Zigzag: left-right-left-right-left
    const xs = [0, 40, 0, 40, 0, 40];
    let fired = false;
    for (const x of xs) {
      t += 40;
      if (det.push(x, 10, t)) fired = true;
    }
    expect(fired).toBe(true);
  });

  it("ignores small jitters", () => {
    let t = 1000;
    const det = new MouseShakeDetector({
      now: () => t,
      minAmplitudePx: 40,
      minReversals: 4,
    });
    const xs = [0, 3, 0, 3, 0, 3, 0];
    let fired = false;
    for (const x of xs) {
      t += 30;
      if (det.push(x, 0, t)) fired = true;
    }
    expect(fired).toBe(false);
  });

  it("respects cooldown after a fire", () => {
    let t = 1000;
    const det = new MouseShakeDetector({
      now: () => t,
      minAmplitudePx: 20,
      minReversals: 3,
      cooldownMs: 2000,
      windowMs: 500,
    });

    const shake = () => {
      const xs = [0, 50, 0, 50, 0];
      for (const x of xs) {
        t += 40;
        if (det.push(x, 0, t)) return true;
      }
      return false;
    };

    expect(shake()).toBe(true);
    t += 100;
    expect(shake()).toBe(false);
    t += 2100;
    expect(shake()).toBe(true);
  });
});

describe("mouse shake preference", () => {
  const store = new Map<string, string>();

  beforeEach(() => {
    store.clear();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => {
        store.set(key, value);
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
      clear: () => store.clear(),
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("defaults to enabled", () => {
    expect(isMouseShakeToolbarEnabled()).toBe(true);
  });

  it("persists disable", () => {
    setMouseShakeToolbarEnabled(false);
    expect(store.get(MOUSE_SHAKE_PREF_KEY)).toBe("0");
    expect(isMouseShakeToolbarEnabled()).toBe(false);
    setMouseShakeToolbarEnabled(true);
    expect(isMouseShakeToolbarEnabled()).toBe(true);
  });
});
