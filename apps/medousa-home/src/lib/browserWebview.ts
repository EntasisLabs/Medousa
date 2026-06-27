/** Iframe helpers (mobile) and embedded browser bounds measurement (desktop). */

export type BrowserWebviewBounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export function canUseNativeBrowserWebview(): boolean {
  return false;
}

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

export async function hideNativeBrowserWebview(): Promise<void> {
  // No-op: desktop uses human_browser Rust shell.
}
