/** Clipboard helpers for vault context actions — never hang the UI thread. */

const CLIPBOARD_TIMEOUT_MS = 2500;

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("clipboard-timeout")), ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (err) => {
        clearTimeout(timer);
        reject(err);
      },
    );
  });
}

function copyViaExecCommand(text: string): boolean {
  if (typeof document === "undefined") return false;
  try {
    const el = document.createElement("textarea");
    el.value = text;
    el.setAttribute("readonly", "");
    el.style.position = "fixed";
    el.style.left = "-9999px";
    el.style.top = "0";
    document.body.appendChild(el);
    el.select();
    const ok = document.execCommand("copy");
    el.remove();
    return ok;
  } catch {
    return false;
  }
}

function clipboardGestureSafe(): boolean {
  // Node / unit tests have no document — allow the timed clipboard path.
  if (typeof document === "undefined") return true;
  // OS overlays (Greenshot, Snipping Tool) steal focus and often lock the
  // clipboard — never touch it while the document is hidden or unfocused.
  if (document.visibilityState === "hidden") return false;
  if (typeof document.hasFocus === "function" && !document.hasFocus()) return false;
  return true;
}

export async function copyTextToClipboard(text: string): Promise<boolean> {
  const payload = text.trim();
  if (!payload) return false;
  if (!clipboardGestureSafe()) return false;
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await withTimeout(navigator.clipboard.writeText(payload), CLIPBOARD_TIMEOUT_MS);
      return true;
    } catch {
      // Fall through to execCommand — WebView2 can hang or deny writeText.
    }
  }
  return copyViaExecCommand(payload);
}

/**
 * Read clipboard text with a hard timeout. On Windows WebView2, `readText`
 * can sit forever behind a permission dialog — never await it unbounded.
 */
export async function readTextFromClipboard(): Promise<string | null> {
  if (typeof navigator === "undefined" || !navigator.clipboard?.readText) {
    return null;
  }
  if (!clipboardGestureSafe()) return null;
  try {
    const text = await withTimeout(navigator.clipboard.readText(), CLIPBOARD_TIMEOUT_MS);
    return text ?? null;
  } catch {
    return null;
  }
}
