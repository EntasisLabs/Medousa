/** Breakpoint aligned with Tailwind `md` — below = mobile shell. */
export const MOBILE_LAYOUT_MAX_WIDTH_PX = 768;

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Native iOS/Android shell — always use mobile UI, not viewport width. */
export function isTauriMobilePlatform(): boolean {
  if (!isTauri() || typeof navigator === "undefined") return false;
  return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
}

/** Tauri desktop build running on macOS (not iOS simulator / Catalyst). */
export function isTauriMacDesktop(): boolean {
  if (!isTauri() || isTauriMobilePlatform()) return false;
  if (typeof navigator === "undefined") return false;
  return /Mac/i.test(navigator.platform) || /Mac OS X/i.test(navigator.userAgent);
}

/** Channel surface tag sent to the daemon for interactive turns from Medousa. */
export function homeChannelSurface(): string {
  if (!isTauri() || typeof navigator === "undefined") return "home-desktop";
  const ua = navigator.userAgent;
  if (/iPhone|iPad|iPod/i.test(ua)) return "home-ios";
  if (/Android/i.test(ua)) return "home-android";
  return "home-desktop";
}

export function isMobileViewport(): boolean {
  if (typeof window === "undefined") return false;
  return window.matchMedia(`(max-width: ${MOBILE_LAYOUT_MAX_WIDTH_PX}px)`).matches;
}

export function shouldUseMobileShell(): boolean {
  return isTauriMobilePlatform() || isMobileViewport();
}

export function watchMobileViewport(onChange: (mobile: boolean) => void): () => void {
  if (typeof window === "undefined") return () => {};

  const query = window.matchMedia(`(max-width: ${MOBILE_LAYOUT_MAX_WIDTH_PX}px)`);
  const handler = () => onChange(query.matches);
  handler();
  query.addEventListener("change", handler);
  return () => query.removeEventListener("change", handler);
}

/**
 * Tag native shells for CSS overrides. Tauri iOS only needs bottom-chrome padding
 * adjusted — do not zero global safe-area vars or change html/body positioning.
 */
export function applyNativeMobileShellLayout(): () => void {
  if (typeof document === "undefined") return () => {};

  const root = document.documentElement;
  if (!isTauriMobilePlatform()) return () => {};

  const ua = navigator.userAgent;
  if (/iPhone|iPad|iPod/i.test(ua)) {
    root.dataset.nativeShell = "ios";
  } else if (/Android/i.test(ua)) {
    root.dataset.nativeShell = "android";
  }

  return () => {
    delete root.dataset.nativeShell;
  };
}
