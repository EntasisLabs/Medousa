/** Shared browser chrome hotkey actions (shell + native embed bridge). */

import {
  dispatchBrowserFocusUrl,
  dispatchBrowserOpenBookmarks,
} from "$lib/utils/browserChromeEvents";
import type { HumanBrowserStore } from "$lib/stores/humanBrowser.svelte";
import { resetContentZoom, stepContentZoom } from "$lib/config/contentZoom";

export type BrowserHotkeyAction =
  | "focusUrl"
  | "find"
  | "bookmarks"
  | "newTab"
  | "reopenTab"
  | "closeTab"
  | "reload"
  | "goBack"
  | "goForward"
  | "zoomIn"
  | "zoomOut"
  | "zoomReset";

export function isBrowserHotkeyAction(value: string): value is BrowserHotkeyAction {
  return (
    value === "focusUrl" ||
    value === "find" ||
    value === "bookmarks" ||
    value === "newTab" ||
    value === "reopenTab" ||
    value === "closeTab" ||
    value === "reload" ||
    value === "goBack" ||
    value === "goForward" ||
    value === "zoomIn" ||
    value === "zoomOut" ||
    value === "zoomReset"
  );
}

export function runBrowserHotkeyAction(
  action: BrowserHotkeyAction,
  store: HumanBrowserStore,
): void {
  switch (action) {
    case "zoomIn":
      stepContentZoom(1);
      return;
    case "zoomOut":
      stepContentZoom(-1);
      return;
    case "zoomReset":
      resetContentZoom();
      return;
    case "focusUrl":
      dispatchBrowserFocusUrl();
      return;
    case "find":
      store.openFindBar();
      return;
    case "bookmarks":
      dispatchBrowserOpenBookmarks();
      return;
    case "newTab":
      void store.openTab();
      return;
    case "reopenTab":
      void store.reopenClosedTab();
      return;
    case "closeTab": {
      const active = store.activeTab;
      if (active) void store.closeTab(active.id);
      return;
    }
    case "reload":
      void store.reload();
      return;
    case "goBack":
      void store.goBack();
      return;
    case "goForward":
      void store.goForward();
      return;
  }
}
