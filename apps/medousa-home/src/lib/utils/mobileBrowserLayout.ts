/** Fallback when `--mobile-bottom-chrome-height` is not measured yet (5.5rem). */
export const MOBILE_BOTTOM_CHROME_DEFAULT = 88;

export type MobileBrowserBounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

type PaddingPx = { top: number; bottom: number; left: number; right: number };

export function readMobileBottomChromeHeight(): number {
  if (typeof document === "undefined") return MOBILE_BOTTOM_CHROME_DEFAULT;
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue("--mobile-bottom-chrome-height")
    .trim();
  if (raw.endsWith("px")) {
    const parsed = parseFloat(raw);
    if (Number.isFinite(parsed) && parsed > 0) return parsed;
  }
  return MOBILE_BOTTOM_CHROME_DEFAULT;
}

/** True once MobileBottomChrome has published a measured height (not 5.5rem fallback). */
export function isMobileBottomChromeMeasured(): boolean {
  return readMobileBottomChromeHeight() < MOBILE_BOTTOM_CHROME_DEFAULT;
}

function rect(el: Element | null | undefined): DOMRect | null {
  return el instanceof HTMLElement ? el.getBoundingClientRect() : null;
}

function readPaddingPx(el: HTMLElement): PaddingPx {
  const style = getComputedStyle(el);
  return {
    top: parseFloat(style.paddingTop) || 0,
    bottom: parseFloat(style.paddingBottom) || 0,
    left: parseFloat(style.paddingLeft) || 0,
    right: parseFloat(style.paddingRight) || 0,
  };
}

/** Live height of the browser URL chrome block (controls + computed padding). */
export function measureBrowserBottomChromeBlock(
  bottomChromeEl: HTMLElement | null | undefined,
): { height: number; padding: PaddingPx } | null {
  if (!bottomChromeEl) return null;
  const box = bottomChromeEl.getBoundingClientRect();
  if (box.height < 1) return null;
  return { height: box.height, padding: readPaddingPx(bottomChromeEl) };
}

/** Primary bounds — content embed host (in-flow / non-native). */
export function measureEmbedHostBounds(
  host: HTMLElement | null | undefined,
): MobileBrowserBounds | null {
  if (!host) return null;
  const r = host.getBoundingClientRect();
  if (r.width < 8 || r.height < 8) return null;
  return { x: r.left, y: r.top, width: r.width, height: r.height };
}

/** Full panel bounds. */
export function measureEmbedPanelBounds(
  panel: HTMLElement | null | undefined,
): MobileBrowserBounds | null {
  if (!panel) return null;
  const r = panel.getBoundingClientRect();
  if (r.width < 8 || r.height < 8) return null;
  return { x: r.left, y: r.top, width: r.width, height: r.height };
}

/** Measured layout inputs for native mobile browser embed. */
export type MobileBrowserEmbedMetrics = {
  bounds: MobileBrowserBounds;
  chromeTop: number | null;
  chromeBlockHeight: number | null;
  chromePaddingTop: number;
  chromePaddingBottom: number;
  controlRowTop: number | null;
  controlRowBottom: number | null;
  tabBarTop: number | null;
  tabBarBottom: number | null;
  chromeBottom: number | null;
  embedHeightFormula: string;
  underlapPx: number | null;
  gapChromeBottomToTab: number | null;
  gapControlRowToTab: number | null;
  shellBottomGap: number | null;
};

/**
 * Native embed height = panel height − browser chrome block + chrome padding-top.
 * All values from getBoundingClientRect / getComputedStyle (no fudge factors).
 */
export function computeMobileBrowserEmbedMetrics(
  panel: HTMLElement | null | undefined,
  bottomChromeEl?: HTMLElement | null,
): MobileBrowserEmbedMetrics | null {
  if (!panel) return null;
  const panelR = panel.getBoundingClientRect();
  if (panelR.width < 8 || panelR.height < 8) return null;

  const bottomEl =
    bottomChromeEl ??
    (document.querySelector("[data-browser-bottom-chrome]") as HTMLElement | null);
  const bottomR = rect(bottomEl);
  const chromeBlock = measureBrowserBottomChromeBlock(bottomEl);
  const padTop = chromeBlock?.padding.top ?? 0;
  const padBottom = chromeBlock?.padding.bottom ?? 0;
  const chromeBlockHeight = chromeBlock?.height ?? bottomR?.height ?? 0;

  const controlRow = bottomEl?.querySelector(
    "[data-browser-controls]",
  ) as HTMLElement | null;
  const controlRowR = rect(controlRow);

  const tabEl = document.querySelector(".mobile-bottom-chrome") as HTMLElement | null;
  const tabR = rect(tabEl);

  const embedHeight = panelR.height - chromeBlockHeight + padTop;
  let embedBottom = panelR.top + embedHeight;
  if (tabR) {
    embedBottom = Math.min(embedBottom, tabR.top);
  }

  const height = embedBottom - panelR.top;
  if (height < 8) return null;

  const viewportBottom =
    window.visualViewport?.height != null
      ? window.visualViewport.height + (window.visualViewport.offsetTop ?? 0)
      : window.innerHeight;

  const controlRowTop = controlRowR?.top ?? null;

  return {
    bounds: {
      x: panelR.left,
      y: panelR.top,
      width: panelR.width,
      height,
    },
    chromeTop: bottomR?.top ?? null,
    chromeBlockHeight: chromeBlockHeight || null,
    chromePaddingTop: padTop,
    chromePaddingBottom: padBottom,
    controlRowTop,
    controlRowBottom: controlRowR?.bottom ?? null,
    tabBarTop: tabR?.top ?? null,
    tabBarBottom: tabR?.bottom ?? null,
    chromeBottom: bottomR?.bottom ?? null,
    embedHeightFormula: "panelHeight - chromeBlock + paddingTop",
    underlapPx: bottomR ? embedBottom - bottomR.top : null,
    gapChromeBottomToTab:
      bottomR && tabR ? tabR.top - bottomR.bottom : null,
    gapControlRowToTab:
      controlRowR && tabR ? tabR.top - controlRowR.bottom : null,
    shellBottomGap: tabR ? viewportBottom - tabR.bottom : null,
  };
}

export function measureMobileBrowserEmbedBounds(
  panel: HTMLElement | null | undefined,
  bottomChromeEl?: HTMLElement | null,
): MobileBrowserBounds | null {
  return computeMobileBrowserEmbedMetrics(panel, bottomChromeEl)?.bounds ?? null;
}
