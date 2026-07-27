import { beforeEach, describe, expect, it, vi } from "vitest";

const checkDaemonHealth = vi.fn();
const startEngine = vi.fn();
const waitForEngine = vi.fn();
const isTauri = vi.fn();
const isTauriMobilePlatform = vi.fn();

vi.mock("$lib/daemon", () => ({
  checkDaemonHealth: (...args: unknown[]) => checkDaemonHealth(...args),
}));

vi.mock("$lib/utils/providersApi", () => ({
  startEngine: (...args: unknown[]) => startEngine(...args),
  waitForEngine: (...args: unknown[]) => waitForEngine(...args),
}));

vi.mock("$lib/window", () => ({
  isTauri: () => isTauri(),
}));

vi.mock("$lib/platform", () => ({
  isTauriMobilePlatform: () => isTauriMobilePlatform(),
}));

import { ensureWorkshopEngineHealthy } from "./ensureWorkshopEngine";

describe("ensureWorkshopEngineHealthy", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    isTauri.mockReturnValue(true);
    isTauriMobilePlatform.mockReturnValue(false);
  });

  it("returns immediately when already healthy", async () => {
    checkDaemonHealth.mockResolvedValue({ ok: true, message: "ready" });
    const health = await ensureWorkshopEngineHealthy();
    expect(health.ok).toBe(true);
    expect(startEngine).not.toHaveBeenCalled();
  });

  it("starts the engine on desktop Tauri when unhealthy", async () => {
    checkDaemonHealth
      .mockResolvedValueOnce({ ok: false, message: "down" })
      .mockResolvedValueOnce({ ok: true, message: "ready" });
    startEngine.mockResolvedValue({ started: true });
    waitForEngine.mockResolvedValue({ ok: true, message: "ready", attempts: 1 });

    const health = await ensureWorkshopEngineHealthy();
    expect(startEngine).toHaveBeenCalledOnce();
    expect(waitForEngine).toHaveBeenCalledOnce();
    expect(health.ok).toBe(true);
  });

  it("does not spawn on mobile or when allowSpawn is false", async () => {
    checkDaemonHealth.mockResolvedValue({ ok: false, message: "down" });
    isTauriMobilePlatform.mockReturnValue(true);
    await ensureWorkshopEngineHealthy();
    expect(startEngine).not.toHaveBeenCalled();

    isTauriMobilePlatform.mockReturnValue(false);
    await ensureWorkshopEngineHealthy({ allowSpawn: false });
    expect(startEngine).not.toHaveBeenCalled();
  });
});
