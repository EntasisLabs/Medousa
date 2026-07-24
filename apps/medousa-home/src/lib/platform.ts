/** Breakpoint aligned with Tailwind `md` — below = mobile shell. */
export const MOBILE_LAYOUT_MAX_WIDTH_PX = 768;

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Native iOS shell (iPhone/iPad Tauri build). */
export function isTauriIos(): boolean {
  if (!isTauri() || typeof navigator === "undefined") return false;
  return /iPhone|iPad|iPod/i.test(navigator.userAgent);
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

/** Tauri desktop (not iOS/Android). */
export function isTauriDesktop(): boolean {
  return isTauri() && !isTauriMobilePlatform();
}

export type TitlebarMode = "overlay-mac" | "custom-winlinux" | "none";

/**
 * How the main window hosts shell tabs in the OS chrome.
 * - overlay-mac: native traffic lights + Overlay title bar
 * - custom-winlinux: frameless + HTML window controls
 * - none: browser / mobile — keep in-content hover tabs
 */
export function titlebarMode(): TitlebarMode {
  if (!isTauriDesktop()) return "none";
  if (isTauriMacDesktop()) return "overlay-mac";
  return "custom-winlinux";
}

/** True when shell tabs live in the unified AppTitlebar. */
export function usesUnifiedTitlebar(): boolean {
  return titlebarMode() !== "none";
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
/** Prefer ⌘ on Apple platforms, Ctrl elsewhere (for shortcut hint labels). */
export function usesMetaModKey(): boolean {
  if (typeof navigator === "undefined") return false;
  // Prefer platform / userAgentData when available; fall back to UA.
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData
      ?.platform ??
    navigator.platform ??
    "";
  if (/Mac|iPhone|iPad|iPod/i.test(platform)) return true;
  return /Mac OS X|Macintosh|iPhone|iPad|iPod/i.test(navigator.userAgent);
}

/** Modifier key glyph/label for UI hints: `⌘` or `Ctrl`. */
export function modKeyLabel(): string {
  return usesMetaModKey() ? "⌘" : "Ctrl";
}

/**
 * Format a shortcut hint for the current platform.
 * Pass the key chord without the primary modifier, e.g. `K`, `⇧B`, `⇧T`.
 * On non-Apple platforms, Shift is spelled out as `Shift+` when `⇧` is used.
 */
export function formatShortcut(chord: string): string {
  const mod = modKeyLabel();
  if (usesMetaModKey()) {
    return `${mod}${chord}`;
  }
  const normalized = chord.replace(/⇧/g, "Shift+").replace(/⌥/g, "Alt+");
  return `${mod}+${normalized}`;
}

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
