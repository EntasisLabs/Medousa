/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { copyCodeText } from "./codeBlockPresentation";
import { copyTextToClipboard } from "$lib/utils/vaultClipboard";

const source = "  function example() {\n\treturn 'hello';\n  }\n\n";
let copied: string | undefined;
beforeEach(() => {
  copied = undefined;
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
  document.execCommand = vi.fn(() => {
    expect(document.activeElement).toBe(document.querySelector("textarea"));
    copied = document.querySelector("textarea")?.value;
    return true;
  });
});
afterEach(() => {
  vi.restoreAllMocks();
  delete (document as Partial<Document>).execCommand;
  vi.unstubAllGlobals();
  vi.useRealTimers();
  document.body.replaceChildren();
});

it("preserves exact code whitespace without changing trimmed vault copies", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  await expect(copyCodeText(source)).resolves.toBe(true);
  expect(writeText).toHaveBeenLastCalledWith(source);
  await copyTextToClipboard("  vault/path  ");
  expect(writeText).toHaveBeenLastCalledWith("vault/path");
});

it.each(["unavailable", "denied", "timeout"])("falls back when the Clipboard API is %s", async (mode) => {
  vi.useFakeTimers();
  const writeText = mode === "denied"
    ? vi.fn().mockRejectedValue(new Error("denied"))
    : vi.fn(() => new Promise<void>(() => {}));
  vi.stubGlobal("navigator", mode === "unavailable" ? {} : { clipboard: { writeText } });
  const button = document.createElement("button");
  document.body.appendChild(button);
  button.focus();
  const pending = copyCodeText(source);
  await vi.advanceTimersByTimeAsync(2500);
  await expect(pending).resolves.toBe(true);
  expect(copied).toBe(source);
  expect(document.querySelector("textarea")).toBeNull();
  expect(document.activeElement).toBe(button);
});

it("cleans up after a failed fallback and reports failure", async () => {
  vi.stubGlobal("navigator", {});
  vi.mocked(document.execCommand).mockImplementation(() => { throw new Error("denied"); });
  await expect(copyCodeText(source)).resolves.toBe(false);
  expect(document.querySelector("textarea")).toBeNull();
});
