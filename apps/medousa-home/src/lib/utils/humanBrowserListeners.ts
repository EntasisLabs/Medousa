/** Tauri event listeners for a scoped human browser store. */

import { listen } from "@tauri-apps/api/event";
import { isTauri } from "$lib/platform";
import type {
  HumanBrowserLoadingPayload,
  HumanBrowserNavigatedPayload,
  HumanBrowserNavStatePayload,
} from "$lib/humanBrowser";
import type { HumanBrowserStore } from "$lib/stores/humanBrowser.svelte";
import type { HumanBrowserSurface } from "$lib/stores/humanBrowserSurface";
import {
  isBrowserHotkeyAction,
  runBrowserHotkeyAction,
} from "$lib/utils/browserHotkeys";

export interface HumanBrowserNewWindowPayload {
  url: string;
  surface?: HumanBrowserSurface;
}

export interface HumanBrowserHotkeyPayload {
  action: string;
  surface?: HumanBrowserSurface;
}

let newWindowGate = { lastUrl: "", lastAt: 0, burst: 0 };

function shouldOpenNewWindowTab(url: string): boolean {
  const now = Date.now();
  if (url === newWindowGate.lastUrl && now - newWindowGate.lastAt < 2500) {
    return false;
  }
  if (now - newWindowGate.lastAt < 400) {
    newWindowGate.burst += 1;
    if (newWindowGate.burst > 2) return false;
  } else {
    newWindowGate.burst = 0;
  }
  newWindowGate.lastUrl = url;
  newWindowGate.lastAt = now;
  return true;
}

function matchesSurface(
  payloadSurface: string | undefined,
  surface: HumanBrowserSurface,
): boolean {
  return (payloadSurface ?? "embed") === surface;
}

export function attachHumanBrowserSurface(
  store: HumanBrowserStore,
  surface: HumanBrowserSurface,
): () => void {
  if (!isTauri()) return () => {};

  const unlistenNav = listen<HumanBrowserNavigatedPayload>(
    "human-browser-navigated",
    (event) => {
      if (!matchesSurface(event.payload.surface, surface)) return;
      store.syncFromNative(event.payload);
    },
  );

  const unlistenLoading = listen<HumanBrowserLoadingPayload>(
    "human-browser-loading",
    (event) => {
      if (!matchesSurface(event.payload.surface, surface)) return;
      store.setLoading(event.payload.loading);
    },
  );

  const unlistenNavState = listen<HumanBrowserNavStatePayload>(
    "human-browser-nav-state",
    (event) => {
      if (!matchesSurface(event.payload.surface, surface)) return;
      store.setNativeNavState(
        event.payload.canGoBack,
        event.payload.canGoForward,
      );
    },
  );

  const unlistenNewWindow = listen<HumanBrowserNewWindowPayload>(
    "human-browser-new-window",
    (event) => {
      if (!matchesSurface(event.payload.surface, surface)) return;
      const url = event.payload.url?.trim();
      if (!url || url === "about:blank") return;
      if (!shouldOpenNewWindowTab(url)) return;
      void store.openTab(url);
    },
  );

  const unlistenHotkey = listen<HumanBrowserHotkeyPayload>(
    "human-browser-hotkey",
    (event) => {
      if (!matchesSurface(event.payload.surface, surface)) return;
      const action = event.payload.action?.trim() ?? "";
      if (!isBrowserHotkeyAction(action)) return;
      runBrowserHotkeyAction(action, store);
    },
  );

  return () => {
    void unlistenNav.then((fn) => fn());
    void unlistenLoading.then((fn) => fn());
    void unlistenNavState.then((fn) => fn());
    void unlistenNewWindow.then((fn) => fn());
    void unlistenHotkey.then((fn) => fn());
  };
}
