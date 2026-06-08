import { isTauriMobilePlatform } from "$lib/platform";

/** Keeps floating mobile composers above the keyboard without shifting the whole shell. */
export function attachMobileKeyboardViewport(
  root: HTMLElement = document.documentElement,
): () => void {
  if (typeof window === "undefined") return () => {};

  let layoutHeight = window.innerHeight;

  const update = () => {
    const viewport = window.visualViewport;
    if (!viewport) {
      root.style.setProperty("--mobile-keyboard-inset", "0px");
      return;
    }

    const inset = Math.max(0, layoutHeight - viewport.height - viewport.offsetTop);
    root.style.setProperty("--mobile-keyboard-inset", `${inset}px`);

    if (inset < 8 && root.dataset.mobileComposerActive !== "true") {
      layoutHeight = window.innerHeight;
    }
  };

  update();
  window.visualViewport?.addEventListener("resize", update);
  window.visualViewport?.addEventListener("scroll", update);
  window.addEventListener("orientationchange", () => {
    layoutHeight = window.innerHeight;
    update();
  });
  window.addEventListener("resize", update);

  return () => {
    window.visualViewport?.removeEventListener("resize", update);
    window.visualViewport?.removeEventListener("scroll", update);
    window.removeEventListener("orientationchange", update);
    window.removeEventListener("resize", update);
    root.style.removeProperty("--mobile-keyboard-inset");
    root.dataset.mobileComposerActive = "false";
  };
}

/** Focus-driven dock mode — tabs hide while typing; keyboard inset lifts bottom chrome. */
export function setMobileComposerFocus(active: boolean) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.mobileComposerActive = active ? "true" : "false";
}

function viewportHeight(): number {
  return window.visualViewport?.height ?? window.innerHeight;
}

function isIosTauriShell(): boolean {
  if (typeof navigator === "undefined") return false;
  return isTauriMobilePlatform() && /iPhone|iPad|iPod/i.test(navigator.userAgent);
}

function bottomChromeAnchor(chromeEl: HTMLElement): HTMLElement {
  const tabBar = chromeEl.querySelector<HTMLElement>(".mobile-tab-bar-inner");
  if (tabBar && getComputedStyle(tabBar).display !== "none") {
    return tabBar;
  }

  const composer = chromeEl.querySelector<HTMLElement>(".mobile-chat-composer");
  return composer ?? chromeEl;
}

/**
 * Measures bottom chrome against the visual viewport and keeps main content aligned.
 * On Tauri iOS, WKWebView often reports safe-area insets that do not match the
 * native layout rect — we detect the actual gap below tabs and pull chrome down.
 */
export function attachMobileBottomChromeLayout(
  chromeEl: HTMLElement | null | undefined,
): () => void {
  if (typeof window === "undefined" || !chromeEl) return () => {};

  const root = document.documentElement;
  let raf = 0;

  const update = () => {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => {
      if (isIosTauriShell()) {
        root.style.setProperty("--mobile-chrome-safe-bottom", "0px");
        const anchor = bottomChromeAnchor(chromeEl);
        const gap = Math.round(viewportHeight() - anchor.getBoundingClientRect().bottom);
        chromeEl.style.bottom = gap > 0 ? `-${gap}px` : "";
      } else {
        chromeEl.style.bottom = "";
      }

      const reserved = Math.max(0, Math.ceil(viewportHeight() - chromeEl.getBoundingClientRect().top));
      root.style.setProperty("--mobile-bottom-chrome-height", `${reserved}px`);
    });
  };

  update();
  const observer = new ResizeObserver(update);
  observer.observe(chromeEl);
  for (const child of chromeEl.children) {
    if (child instanceof HTMLElement) observer.observe(child);
  }
  window.visualViewport?.addEventListener("resize", update);
  window.visualViewport?.addEventListener("scroll", update);
  window.addEventListener("orientationchange", update);

  return () => {
    cancelAnimationFrame(raf);
    observer.disconnect();
    window.visualViewport?.removeEventListener("resize", update);
    window.visualViewport?.removeEventListener("scroll", update);
    window.removeEventListener("orientationchange", update);
    chromeEl.style.bottom = "";
    root.style.removeProperty("--mobile-bottom-chrome-height");
  };
}
