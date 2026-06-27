/** Browser pane measurement — native embed (Tauri desktop) vs iframe (mobile webviews). */

import { isTauri, isTauriIos, isTauriMobilePlatform } from "$lib/platform";

export type BrowserWebviewBounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

/** Tauri desktop (including narrow viewport / mobile shell) or native iOS WKWebView overlay. */
export function canUseNativeBrowserWebview(): boolean {
  return isTauri() && (!isTauriMobilePlatform() || isTauriIos());
}

/** Plain DOM rect for native webview placement — no visualViewport offsets. */
export function measureBrowserWebviewBounds(el: HTMLElement): BrowserWebviewBounds {
  const rect = el.getBoundingClientRect();
  return {
    x: Math.round(rect.left),
    y: Math.round(rect.top),
    width: Math.max(8, Math.round(rect.width)),
    height: Math.max(8, Math.round(rect.height)),
  };
}

/** Iframe sizing on iOS/Android — account for visual viewport keyboard inset. */
export function measureBrowserPaneForIframe(el: HTMLElement): BrowserWebviewBounds {
  const rect = el.getBoundingClientRect();
  const viewport = window.visualViewport;
  const offsetLeft = viewport?.offsetLeft ?? 0;
  const offsetTop = viewport?.offsetTop ?? 0;
  const top = rect.top + offsetTop;
  const left = rect.left + offsetLeft;
  const viewportBottom = offsetTop + (viewport?.height ?? window.innerHeight);
  const height = Math.max(8, Math.min(rect.height, viewportBottom - top));

  return {
    x: Math.round(left),
    y: Math.round(top),
    width: Math.max(8, Math.round(rect.width)),
    height: Math.max(8, Math.round(height)),
  };
}

export function isPaneLayoutReady(bounds: BrowserWebviewBounds): boolean {
  return bounds.width >= 8 && bounds.height >= 120;
}
