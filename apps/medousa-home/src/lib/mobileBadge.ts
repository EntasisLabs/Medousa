/** App icon badging (PWA / Chromium) when tray is unavailable on mobile. */

export async function setMobileBadge(count: number): Promise<void> {
  if (typeof navigator === "undefined") return;
  const nav = navigator as Navigator & {
    setAppBadge?: (count?: number) => Promise<void>;
    clearAppBadge?: () => Promise<void>;
  };
  if (!nav.setAppBadge) return;
  try {
    if (count > 0) {
      await nav.setAppBadge(count);
    } else if (nav.clearAppBadge) {
      await nav.clearAppBadge();
    } else {
      await nav.setAppBadge(0);
    }
  } catch {
    // Unsupported platform — ignore.
  }
}
