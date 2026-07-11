/** Cross-component browser chrome hotkey / action events. */

export const BROWSER_OPEN_BOOKMARKS_EVENT = "medousa-browser-open-bookmarks";
export const BROWSER_FOCUS_URL_EVENT = "medousa-browser-focus-url";

export function dispatchBrowserOpenBookmarks() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(BROWSER_OPEN_BOOKMARKS_EVENT));
}

export function dispatchBrowserFocusUrl() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(BROWSER_FOCUS_URL_EVENT));
}
