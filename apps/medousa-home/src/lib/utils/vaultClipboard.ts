/** Clipboard helpers for vault context actions. */

export async function copyTextToClipboard(text: string): Promise<boolean> {
  const payload = text.trim();
  if (!payload) return false;
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(payload);
      return true;
    } catch {
      return false;
    }
  }
  return false;
}
