/** Mobile browser chrome heights — must stay in sync with human_browser.rs */

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

/** Dev-only: compare Rust top chrome vs DOM content pane (enable via html[data-layout-debug]). */
export function logMobileBrowserLayoutDebug(): void {
  if (typeof document === "undefined") return;
  if (!document.documentElement.hasAttribute("data-layout-debug")) return;

  const surface = document.querySelector("[data-browser-surface]");
  const rect = surface?.getBoundingClientRect();
  const bottomChrome = readMobileBottomChromeHeight();

  console.info("[mobile-browser-layout]", {
    rustTopChrome: MOBILE_WEB_CHROME_HEIGHT,
    surfaceTop: rect?.top ?? null,
    surfaceHeight: rect?.height ?? null,
    deltaTop: rect != null ? rect.top - MOBILE_WEB_CHROME_HEIGHT : null,
    bottomChrome,
    windowInner: { w: window.innerWidth, h: window.innerHeight },
  });
}
