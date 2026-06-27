/** Mobile browser chrome heights — must stay in sync with human_browser.rs */

import type { HumanBrowserEmbedBounds } from "$lib/humanBrowser";

/** `h-[52px]` BrowserPanel URL/toolbar row (single chrome owner on Web tab). */
export const MOBILE_WEB_CHROME_HEIGHT = 52;

/** Fallback when `--mobile-bottom-chrome-height` is not measured yet (5.5rem). */
export const MOBILE_BOTTOM_CHROME_DEFAULT = 88;

export function readMobileBottomChromeHeight(): number {
  if (typeof document === "undefined") return MOBILE_BOTTOM_CHROME_DEFAULT;
  const raw = getComputedStyle(document.documentElement).getPropertyValue(
    "--mobile-bottom-chrome-height",
  );
  const parsed = parseFloat(raw);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : MOBILE_BOTTOM_CHROME_DEFAULT;
}

/** Measure the Web tab content pane — plain viewport coords, no visualViewport offset. */
export function measureMobileBrowserSurfaceBounds(): HumanBrowserEmbedBounds | null {
  if (typeof document === "undefined") return null;
  const surface = document.querySelector("[data-browser-surface]");
  if (!(surface instanceof HTMLElement)) return null;

  const rect = surface.getBoundingClientRect();
  if (rect.width < 8 || rect.height < 8) return null;

  return {
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
  };
}

/** Dev-only: compare Rust top chrome vs DOM content pane (enable via html[data-layout-debug]). */
export function logMobileBrowserLayoutDebug(measured: HumanBrowserEmbedBounds | null): void {
  if (typeof document === "undefined") return;
  if (!document.documentElement.hasAttribute("data-layout-debug")) return;

  console.info("[mobile-browser-layout]", {
    rustTopChrome: MOBILE_WEB_CHROME_HEIGHT,
    measured,
    bottomChrome: readMobileBottomChromeHeight(),
    windowInner: { w: window.innerWidth, h: window.innerHeight },
  });
}
