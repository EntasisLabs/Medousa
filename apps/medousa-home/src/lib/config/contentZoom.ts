/**
 * VS Code–style content zoom for notes / chats / scripts.
 * Sets `--content-zoom` on <html>; surfaces multiply it into font-size.
 * Do not apply CSS `zoom` to scroll hosts — that breaks Live typing/scroll.
 */

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

export function applyContentZoomCss(value: number = readContentZoom()): number {
  const zoom = clampContentZoom(value);
  if (typeof document !== "undefined") {
    document.documentElement.style.setProperty("--content-zoom", String(zoom));
  }
  return zoom;
}

export function contentZoomPercent(value: number = readContentZoom()): string {
  return `${Math.round(clampContentZoom(value) * 100)}%`;
}

/** Step zoom; persists + applies CSS. Returns the new factor. */
export function stepContentZoom(deltaSteps: number): number {
  const next = clampContentZoom(readContentZoom() + deltaSteps * CONTENT_ZOOM_STEP);
  writeContentZoom(next);
  return applyContentZoomCss(next);
}

export function resetContentZoom(): number {
  writeContentZoom(CONTENT_ZOOM_DEFAULT);
  return applyContentZoomCss(CONTENT_ZOOM_DEFAULT);
}
