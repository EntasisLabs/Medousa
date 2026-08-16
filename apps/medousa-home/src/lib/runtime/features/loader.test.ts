import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FEATURE_CATALOG } from "./catalog";
import {
  FeatureLoadError,
  disposeFeature,
  loadFeature,
  loadedFeature,
  resetFeaturesForTests,
} from "./loader";
import type { FeatureInstance, FeatureModule } from "./types";

const dir = dirname(fileURLToPath(import.meta.url));

afterEach(() => {
  resetFeaturesForTests();
});

describe("feature catalog", () => {
  it("does not import stores or Svelte implementations", () => {
    const source = readFileSync(join(dir, "catalog.ts"), "utf8");
    expect(source).not.toMatch(/\$lib\/stores/);
    expect(source).not.toMatch(/from ["'][^"']+\.svelte["']/);
    expect(FEATURE_CATALOG.some((entry) => entry.id === "shell-desktop")).toBe(true);
    expect(FEATURE_CATALOG.some((entry) => entry.id === "shell-mobile")).toBe(true);
    expect(
      FEATURE_CATALOG.every((entry) => entry.preload === "never" || entry.preload === "intent"),
    ).toBe(true);
  });
});

describe("feature loader", () => {
  it("dedupes concurrent loads of the same feature", async () => {
    let starts = 0;
    const importModule = async (): Promise<FeatureModule> => ({
      async start() {
        starts += 1;
        await Promise.resolve();
        return { dispose() {} };
      },
    });
    const [a, b] = await Promise.all([
      loadFeature("spotlight", importModule, { platform: "desktop" }),
      loadFeature("spotlight", importModule, { platform: "desktop" }),
    ]);
    expect(a).toBe(b);
    expect(starts).toBe(1);
    expect(loadedFeature("spotlight")).toBe(a);
  });

  it("disposes a tracked instance when start throws", async () => {
    const disposed: string[] = [];
    const flaky = async (): Promise<FeatureModule> => ({
      async start(context) {
        const created: FeatureInstance = {
          dispose(reason) {
            disposed.push(reason);
          },
        };
        context.track(created);
        await Promise.resolve();
        throw new Error("bind failed");
      },
    });
    await expect(loadFeature("wizard", flaky, { platform: "desktop" })).rejects.toBeInstanceOf(
      FeatureLoadError,
    );
    expect(disposed).toEqual(["start-failed"]);
    expect(loadedFeature("wizard")).toBeUndefined();
  });

  it("cancels an inflight load and disposes a started instance", async () => {
    const disposed: string[] = [];
    const ac = new AbortController();
    let releaseStart: () => void = () => {};
    let started = false;
    const gate = new Promise<void>((resolve) => {
      releaseStart = resolve;
    });
    const importModule = async (): Promise<FeatureModule> => ({
      async start() {
        started = true;
        await gate;
        return {
          dispose(reason) {
            disposed.push(reason);
          },
        };
      },
    });
    const pending = loadFeature("browser", importModule, {
      platform: "mobile",
      signal: ac.signal,
    });
    await vi.waitFor(() => expect(started).toBe(true));
    ac.abort();
    releaseStart();
    await expect(pending).rejects.toMatchObject({ reason: "cancelled" });
    expect(disposed).toEqual(["cancelled"]);
    expect(loadedFeature("browser")).toBeUndefined();
  });

  it("does not cancel a shared load while another waiter remains", async () => {
    const cancelled = new AbortController();
    let releaseStart: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      releaseStart = resolve;
    });
    const importModule = async (): Promise<FeatureModule> => ({
      async start() {
        await gate;
        return { dispose() {} };
      },
    });
    const first = loadFeature("browser", importModule, {
      platform: "desktop",
      signal: cancelled.signal,
    });
    const second = loadFeature("browser", importModule, { platform: "desktop" });

    cancelled.abort("navigation");
    releaseStart();

    await expect(first).rejects.toMatchObject({ reason: "cancelled" });
    await expect(second).resolves.toBe(loadedFeature("browser"));
  });

  it("disposeFeature tears down a live instance", async () => {
    const disposed: string[] = [];
    await loadFeature(
      "settings",
      async () => ({
        async start() {
          return {
            dispose(reason) {
              disposed.push(reason);
            },
          };
        },
      }),
      { platform: "desktop" },
    );
    await disposeFeature("settings", "teardown");
    expect(disposed).toEqual(["teardown"]);
    expect(loadedFeature("settings")).toBeUndefined();
  });
});
