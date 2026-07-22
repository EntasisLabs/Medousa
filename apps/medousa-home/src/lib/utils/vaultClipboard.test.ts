import { afterEach, describe, expect, it, vi } from "vitest";
import { copyTextToClipboard, readTextFromClipboard } from "./vaultClipboard";

describe("vaultClipboard", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
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

  it("skips clipboard while the document is unfocused (OS overlay)", async () => {
    const writeText = vi.fn(async () => {});
    const readText = vi.fn(async () => "secret");
    vi.stubGlobal("navigator", { clipboard: { writeText, readText } });
    vi.stubGlobal("document", {
      visibilityState: "visible",
      hasFocus: () => false,
    });
    await expect(copyTextToClipboard("hello")).resolves.toBe(false);
    await expect(readTextFromClipboard()).resolves.toBeNull();
    expect(writeText).not.toHaveBeenCalled();
    expect(readText).not.toHaveBeenCalled();
  });
});
