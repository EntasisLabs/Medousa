/** Shared localStorage handoff for the custom-view OS pop-out. */
export const VIEW_POPOUT_SURFACE_KEY = "medousa-home-view-popout-surface";
export const VIEW_POPOUT_LAST_KEY = "medousa-home-view-popout-last";

export function writeViewPopoutSurface(surfaceId: string): void {
  const trimmed = surfaceId.trim();
  if (!trimmed || typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(VIEW_POPOUT_SURFACE_KEY, trimmed);
    localStorage.setItem(VIEW_POPOUT_LAST_KEY, trimmed);
  } catch {
    /* ignore quota / private mode */
  }
}

export function readViewPopoutSurface(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(VIEW_POPOUT_SURFACE_KEY);
    const trimmed = raw?.trim() ?? "";
    return trimmed || null;
  } catch {
    return null;
  }
}

export function readLastViewPopoutSurface(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(VIEW_POPOUT_LAST_KEY);
    const trimmed = raw?.trim() ?? "";
    return trimmed || null;
  } catch {
    return null;
  }
}

export function clearViewPopoutSurface(): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.removeItem(VIEW_POPOUT_SURFACE_KEY);
  } catch {
    /* ignore */
  }
}
