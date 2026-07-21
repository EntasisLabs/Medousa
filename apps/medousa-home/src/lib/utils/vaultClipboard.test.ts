import { afterEach, describe, expect, it, vi } from "vitest";
import { copyTextToClipboard, readTextFromClipboard } from "./vaultClipboard";

describe("vaultClipboard", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("copyTextToClipboard writes via navigator.clipboard", async () => {
    const writeText = vi.fn(async () => {});
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    await expect(copyTextToClipboard("hello")).resolves.toBe(true);
    expect(writeText).toHaveBeenCalledWith("hello");
  });

  it("readTextFromClipboard times out instead of hanging", async () => {
    vi.useFakeTimers();
    const readText = vi.fn(() => new Promise<string>(() => {}));
    vi.stubGlobal("navigator", { clipboard: { readText } });
    const pending = readTextFromClipboard();
    await vi.advanceTimersByTimeAsync(3000);
    await expect(pending).resolves.toBeNull();
  });
});
