/** Sync embedded native browser webview bounds (desktop Tauri only). */

import { invoke } from "@tauri-apps/api/core";
import { isTauri, isTauriMobilePlatform } from "$lib/platform";

export type BrowserWebviewBounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export function canUseNativeBrowserWebview(): boolean {
  return isTauri() && !isTauriMobilePlatform();
}

/** Measure a DOM element for native webview placement. */
export function measureBrowserWebviewBounds(el: HTMLElement): BrowserWebviewBounds {
  const rect = el.getBoundingClientRect();
  const viewport = window.visualViewport;

  // iOS / mobile WKWebView: visual viewport can be offset from the layout viewport.
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

export async function syncNativeBrowserWebview(
  bounds: BrowserWebviewBounds,
  visible: boolean,
  initialUrl?: string | null,
): Promise<void> {
  if (!canUseNativeBrowserWebview()) return;
  await invoke("browser_webview_sync", {
    bounds,
    visible,
    url: initialUrl ?? null,
  });
}

export async function navigateNativeBrowserWebview(url: string): Promise<void> {
  if (!canUseNativeBrowserWebview()) return;
  await invoke("browser_webview_navigate", { url });
}

export async function hideNativeBrowserWebview(): Promise<void> {
  if (!canUseNativeBrowserWebview()) return;
  await invoke("browser_webview_hide");
}

export async function reloadNativeBrowserWebview(): Promise<void> {
  if (!canUseNativeBrowserWebview()) return;
  await invoke("browser_webview_reload");
}

export async function nativeBrowserGoBack(): Promise<void> {
  if (!canUseNativeBrowserWebview()) return;
  await invoke("browser_webview_go_back");
}

export async function nativeBrowserGoForward(): Promise<void> {
  if (!canUseNativeBrowserWebview()) return;
  await invoke("browser_webview_go_forward");
}
