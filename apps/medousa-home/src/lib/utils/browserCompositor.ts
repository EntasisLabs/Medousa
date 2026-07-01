/**
 * Unified native browser embed lifecycle — DOM-measured bounds + visibility policy.
 */

import {
  humanBrowserEmbedApplyMobileLayout,
  humanBrowserEmbedHide,
  humanBrowserEmbedReadBounds,
  humanBrowserEmbedSetBounds,
  humanBrowserEmbedShow,
  humanBrowserNavigate,
  humanBrowserSetMobileShellActive,
  type HumanBrowserEmbedBounds,
} from "$lib/humanBrowser";
import { measureBrowserWebviewBounds } from "$lib/browserWebview";
import { getBrowserPopoverOverlayDepth } from "$lib/utils/browserPopoverOverlay";
import {
  isMobileBottomChromeMeasured,
  measureDesktopBrowserEmbedBounds,
  measureEmbedHostBounds,
  measureNativeBrowserEmbedBounds,
  readMobileBottomChromeHeight,
} from "$lib/utils/mobileBrowserLayout";
import { isShellLayoutDebugEnabled } from "$lib/utils/shellLayoutDebug";

export type BrowserCompositorMode = "desktop" | "mobile";

export type BrowserCompositorHideReason =
  | "inactive"
  | "startPage"
  | "popover"
  | "urlFocus"
  | "manual"
  | "shellDebug";

export type BrowserCompositorState = {
  hideReasons: BrowserCompositorHideReason[];
  lastBounds: HumanBrowserEmbedBounds | null;
  visible: boolean;
  /** Native readback after layout (DEV). */
  nativeReadback?: (HumanBrowserEmbedBounds & {
    windowWidth: number;
    windowHeight: number;
    shellOriginX: number;
    shellOriginY: number;
  }) | null;
  readbackError?: string | null;
};

export type BrowserCompositorOptions = {
  mode: BrowserCompositorMode;
  /** When false, compositor detaches and hides embed. */
  getActive: () => boolean;
  getShowStartPage: () => boolean;
  /** Popover overlay depth — defaults to browserPopoverOverlay module. */
  getPopoverDepth?: () => number;
  /** Mobile: hide while URL bar focused (keyboard up). */
  getUrlBarFocused?: () => boolean;
  /** Mobile: re-navigate after webview recreation. */
  getActiveUrl?: () => string | null;
  /** Called after layout with debug info (DEV overlay). */
  onStateChange?: (state: BrowserCompositorState) => void;
};

function isCompositorDebugEnabled(): boolean {
  return (
    import.meta.env.DEV &&
    typeof localStorage !== "undefined" &&
    localStorage.getItem("medousa-browser-compositor-debug") === "1"
  );
}

function measureBounds(
  mode: BrowserCompositorMode,
  hostEl: HTMLElement,
  panelEl: HTMLElement | null,
  bottomChromeEl: HTMLElement | null,
  chromeEl: HTMLElement | null,
): HumanBrowserEmbedBounds | null {
  if (mode === "mobile" && panelEl) {
    if (!isMobileBottomChromeMeasured()) return null;
    const native = measureNativeBrowserEmbedBounds(panelEl, bottomChromeEl);
    if (native) return native;
  }
  if (mode === "desktop") {
    const desktop = measureDesktopBrowserEmbedBounds(
      panelEl ?? hostEl,
      chromeEl ?? hostEl,
      hostEl,
    );
    if (desktop) return desktop;
  }
  const measured = measureEmbedHostBounds(hostEl) ?? measureBrowserWebviewBounds(hostEl);
  if (measured.width < 8 || measured.height < 8) return null;
  return measured;
}

function resolveHideReasons(options: BrowserCompositorOptions): BrowserCompositorHideReason[] {
  const reasons: BrowserCompositorHideReason[] = [];
  if (!options.getActive()) reasons.push("inactive");
  if (options.getShowStartPage()) reasons.push("startPage");
  const popoverDepth = options.getPopoverDepth?.() ?? getBrowserPopoverOverlayDepth();
  if (popoverDepth > 0) reasons.push("popover");
  if (options.getUrlBarFocused?.()) reasons.push("urlFocus");
  if (isShellLayoutDebugEnabled()) reasons.push("shellDebug");
  return reasons;
}

function boundsEqual(
  a: HumanBrowserEmbedBounds | null,
  b: HumanBrowserEmbedBounds,
): boolean {
  if (!a) return false;
  return (
    Math.round(a.x) === Math.round(b.x) &&
    Math.round(a.y) === Math.round(b.y) &&
    Math.round(a.width) === Math.round(b.width) &&
    Math.round(a.height) === Math.round(b.height)
  );
}

export type BrowserCompositor = {
  attach: (elements: {
    hostEl: HTMLElement;
    panelEl?: HTMLElement | null;
    bottomChromeEl?: HTMLElement | null;
    chromeEl?: HTMLElement | null;
  }) => void;
  detach: () => void;
  scheduleLayout: () => void;
  flushLayout: () => Promise<void>;
  /** Popover overlay hooks — prefer over direct embed hide/show. */
  pushPopoverHide: () => void;
  popPopoverHide: () => void;
  getState: () => BrowserCompositorState;
};

export function createBrowserCompositor(
  options: BrowserCompositorOptions,
): BrowserCompositor {
  let hostEl: HTMLElement | null = null;
  let panelEl: HTMLElement | null = null;
  let bottomChromeEl: HTMLElement | null = null;
  let chromeEl: HTMLElement | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let manualHide = false;
  let layoutRaf: number | null = null;
  let layoutGeneration = 0;
  let layoutChain: Promise<void> = Promise.resolve();
  let lastBounds: HumanBrowserEmbedBounds | null = null;
  let lastVisible = false;
  let resizeListenerBound = false;
  let tauriResizeUnlisten: (() => void) | null = null;

  function currentState(): BrowserCompositorState {
    const hideReasons = resolveHideReasons(options);
    if (manualHide) hideReasons.push("manual");
    return {
      hideReasons,
      lastBounds,
      visible: lastVisible,
    };
  }

  function notifyState() {
    options.onStateChange?.(currentState());
  }

  async function attachDebugReadback(
    gen: number,
    state: BrowserCompositorState,
  ): Promise<void> {
    if (!isCompositorDebugEnabled()) return;
    try {
      const readback = await humanBrowserEmbedReadBounds();
      if (gen !== layoutGeneration) return;
      options.onStateChange?.({
        ...state,
        nativeReadback: readback,
        readbackError: null,
      });
    } catch (err) {
      if (gen !== layoutGeneration) return;
      options.onStateChange?.({
        ...state,
        nativeReadback: null,
        readbackError: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function applyLayoutOnce(): Promise<void> {
    const gen = ++layoutGeneration;
    if (!hostEl || !options.getActive()) {
      await humanBrowserEmbedHide().catch(() => {});
      lastVisible = false;
      notifyState();
      return;
    }

    const hideReasons = resolveHideReasons(options);
    if (manualHide) hideReasons.push("manual");

    if (hideReasons.length > 0) {
      const bounds = measureBounds(
        options.mode,
        hostEl,
        panelEl,
        bottomChromeEl,
        chromeEl,
      );
      if (bounds) lastBounds = bounds;
      await humanBrowserEmbedHide().catch(() => {});
      lastVisible = false;
      const hiddenState = currentState();
      notifyState();
      await attachDebugReadback(gen, hiddenState);
      return;
    }

    const bounds = measureBounds(
      options.mode,
      hostEl,
      panelEl,
      bottomChromeEl,
      chromeEl,
    );
    if (!bounds || gen !== layoutGeneration) return;

    const boundsChanged = !boundsEqual(lastBounds, bounds);

    if (options.mode === "mobile") {
      lastBounds = bounds;
      await humanBrowserSetMobileShellActive(true);
      if (gen !== layoutGeneration) return;
      const recreated = await humanBrowserEmbedApplyMobileLayout({
        bottomChromeHeight: readMobileBottomChromeHeight(),
        contentBounds: bounds,
      });
      await humanBrowserEmbedShow();
      if (gen !== layoutGeneration) return;

      const url = options.getActiveUrl?.();
      if (recreated && url && url !== "about:blank") {
        await humanBrowserNavigate(url);
      }

      if (recreated && options.getShowStartPage() === false) {
        const readback = await humanBrowserEmbedReadBounds().catch(() => null);
        if (readback && gen === layoutGeneration) {
          const gapY = Math.abs(readback.y - bounds.y);
          const gapH = Math.abs(readback.height - bounds.height);
          if (gapY > 2 || gapH > 2) {
            await humanBrowserEmbedApplyMobileLayout({
              bottomChromeHeight: readMobileBottomChromeHeight(),
              contentBounds: bounds,
            });
            await humanBrowserEmbedShow();
          }
        }
      }
    } else {
      await humanBrowserSetMobileShellActive(false);
      if (gen !== layoutGeneration) return;
      const wasHidden = !lastVisible;
      if (boundsChanged) {
        await humanBrowserEmbedSetBounds(bounds);
        lastBounds = bounds;
        await humanBrowserEmbedShow();
        lastVisible = true;
      } else if (!lastVisible) {
        lastBounds = bounds;
        await humanBrowserEmbedShow();
        lastVisible = true;
      }
      if (wasHidden && lastVisible && gen === layoutGeneration) {
        const url = options.getActiveUrl?.();
        if (url && url !== "about:blank") {
          await humanBrowserNavigate(url);
        }
      }
    }
    if (options.mode === "mobile") {
      lastVisible = true;
    }
    const visibleState = currentState();
    notifyState();
    await attachDebugReadback(gen, visibleState);

    if (import.meta.env.DEV && options.mode === "mobile") {
      const readback = await humanBrowserEmbedReadBounds().catch(() => null);
      if (readback && gen === layoutGeneration) {
        const gapY = Math.abs(readback.y - bounds.y);
        const gapH = Math.abs(readback.height - bounds.height);
        if (gapY > 2 || gapH > 2) {
          console.debug("[browserCompositor] readback mismatch", { bounds, readback });
        }
      }
    }
  }

  async function ensureTauriResizeListener() {
    if (tauriResizeUnlisten) return;
    if (typeof window === "undefined" || !("__TAURI__" in window)) return;
    const { listen } = await import("@tauri-apps/api/event");
    tauriResizeUnlisten = await listen("human-browser-window-resized", () => {
      scheduleLayout();
    });
  }

  function scheduleLayout() {
    if (layoutRaf != null) cancelAnimationFrame(layoutRaf);
    layoutRaf = requestAnimationFrame(() => {
      layoutRaf = null;
      layoutChain = layoutChain.then(() => applyLayoutOnce()).catch(() => {});
    });
  }

  function attach(elements: {
    hostEl: HTMLElement;
    panelEl?: HTMLElement | null;
    bottomChromeEl?: HTMLElement | null;
    chromeEl?: HTMLElement | null;
  }) {
    const hostChanged = hostEl !== elements.hostEl;
    hostEl = elements.hostEl;
    panelEl = elements.panelEl ?? null;
    bottomChromeEl = elements.bottomChromeEl ?? null;
    chromeEl = elements.chromeEl ?? null;

    if (!hostChanged && resizeObserver) {
      scheduleLayout();
      return;
    }

    resizeObserver?.disconnect();
    resizeObserver = new ResizeObserver(() => scheduleLayout());
    resizeObserver.observe(hostEl);
    if (chromeEl) resizeObserver.observe(chromeEl);
    if (panelEl) resizeObserver.observe(panelEl);
    if (bottomChromeEl) resizeObserver.observe(bottomChromeEl);

    const tabBar = document.querySelector(".mobile-bottom-chrome");
    if (tabBar instanceof HTMLElement) {
      resizeObserver.observe(tabBar);
    }

    if (!resizeListenerBound) {
      window.addEventListener("resize", scheduleLayout);
      resizeListenerBound = true;
    }
    void ensureTauriResizeListener();
    scheduleLayout();
  }

  function detach() {
    if (layoutRaf != null) {
      cancelAnimationFrame(layoutRaf);
      layoutRaf = null;
    }
    resizeObserver?.disconnect();
    resizeObserver = null;
    if (resizeListenerBound) {
      window.removeEventListener("resize", scheduleLayout);
      resizeListenerBound = false;
    }
    tauriResizeUnlisten?.();
    tauriResizeUnlisten = null;
    hostEl = null;
    panelEl = null;
    bottomChromeEl = null;
    chromeEl = null;
    void humanBrowserEmbedHide().catch(() => {});
    lastVisible = false;
    notifyState();
  }

  return {
    attach,
    detach,
    scheduleLayout,
    flushLayout: () => layoutChain.then(() => applyLayoutOnce()),
    getState: currentState,
  };
}

/** Module-level compositor instance for popover overlay integration. */
let sharedCompositor: BrowserCompositor | null = null;

export function registerBrowserCompositor(compositor: BrowserCompositor | null) {
  sharedCompositor = compositor;
}

export function getBrowserCompositor(): BrowserCompositor | null {
  return sharedCompositor;
}
