/**
 * VS Code–style UI zoom via Tauri's native webview zoom
 * (`getCurrentWebview().setZoom`). That scales the page correctly inside the
 * window — unlike CSS `zoom`, which crops or leaves empty bands.
 *
 * Persist factor in localStorage; re-apply on shell mount.
 * Keep `--content-zoom` at 1 so legacy font multipliers don't double-scale.
 * Mobile / non-Tauri: no-op (API unsupported).
 */

import { isTauri } from "$lib/platform";

const STORAGE_KEY = "medousa-home-content-zoom";
export const CONTENT_ZOOM_MIN = 0.7;
export const CONTENT_ZOOM_MAX = 1.6;
export const CONTENT_ZOOM_STEP = 0.1;
export const CONTENT_ZOOM_DEFAULT = 1;

export function clampContentZoom(value: number): number {
  if (!Number.isFinite(value)) return CONTENT_ZOOM_DEFAULT;
  const stepped = Math.round(value / CONTENT_ZOOM_STEP) * CONTENT_ZOOM_STEP;
  return Math.min(
    CONTENT_ZOOM_MAX,
    Math.max(CONTENT_ZOOM_MIN, Number(stepped.toFixed(2))),
  );
}

export function readContentZoom(): number {
  if (typeof localStorage === "undefined") return CONTENT_ZOOM_DEFAULT;
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw == null) return CONTENT_ZOOM_DEFAULT;
  const n = Number(raw);
  return clampContentZoom(n);
}

export function writeContentZoom(value: number): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, String(clampContentZoom(value)));
}

/** Strip any leftover CSS zoom hacks from earlier experiments. */
function clearCssZoomHacks(root: HTMLElement): void {
  root.style.zoom = "";
  root.style.width = "";
  root.style.height = "";
  root.style.removeProperty("--ui-zoom");
  root.style.setProperty("--content-zoom", "1");
}

async function applyNativeWebviewZoom(zoom: number): Promise<void> {
  if (!isTauri()) return;
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().setZoom(zoom);
  } catch {
    // Unsupported on mobile / missing ACL — leave layout alone.
  }
}

/**
 * Persist-applied zoom factor. Clears CSS hacks and asks Tauri to set webview zoom.
 * Returns the clamped factor synchronously (native apply is async).
 */
export function applyContentZoomCss(value: number = readContentZoom()): number {
  const zoom = clampContentZoom(value);
  if (typeof document !== "undefined") {
    clearCssZoomHacks(document.documentElement);
  }
  void applyNativeWebviewZoom(zoom);
  return zoom;
}

export function contentZoomPercent(value: number = readContentZoom()): string {
  return `${Math.round(clampContentZoom(value) * 100)}%`;
}

/** Step zoom; persists + applies. Returns the new factor. */
export function stepContentZoom(deltaSteps: number): number {
  const next = clampContentZoom(readContentZoom() + deltaSteps * CONTENT_ZOOM_STEP);
  writeContentZoom(next);
  return applyContentZoomCss(next);
}

export function resetContentZoom(): number {
  writeContentZoom(CONTENT_ZOOM_DEFAULT);
  return applyContentZoomCss(CONTENT_ZOOM_DEFAULT);
}
