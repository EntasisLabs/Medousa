/** Global fan-out for human webview navigation → agent metadata stores. */

import { listen } from "@tauri-apps/api/event";
import { isTauri } from "$lib/platform";
import type {
  HumanBrowserLoadingPayload,
  HumanBrowserNavigatedPayload,
  HumanBrowserNavStatePayload,
} from "$lib/humanBrowser";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { browser } from "$lib/stores/browser.svelte";
import { browserContext } from "$lib/stores/browserContext.svelte";

let agentNavigationInFlight = false;

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

export interface HumanBrowserNewWindowPayload {
  url: string;
}

/** Call before agent-initiated humanBrowser.navigate to avoid flipping control to user. */
export function markAgentNavigation() {
  agentNavigationInFlight = true;
}

export function attachAgentBrowserCoord(): () => void {
  if (!isTauri()) return () => {};

  const unlistenNav = listen<HumanBrowserNavigatedPayload>("human-browser-navigated", (event) => {
    humanBrowser.syncFromNative(event.payload);
    browserContext.applyPayload(event.payload);
    void browser.syncFromNative(event.payload.url);

    if (agentNavigationInFlight) {
      agentNavigationInFlight = false;
      return;
    }
    if (browser.control === "agent") {
      void browser.setControl("user");
    }
  });

  const unlistenLoading = listen<HumanBrowserLoadingPayload>(
    "human-browser-loading",
    (event) => {
      humanBrowser.setLoading(event.payload.loading);
    },
  );

  const unlistenNavState = listen<HumanBrowserNavStatePayload>(
    "human-browser-nav-state",
    (event) => {
      humanBrowser.setNativeNavState(
        event.payload.canGoBack,
        event.payload.canGoForward,
      );
    },
  );

  const unlistenNewWindow = listen<HumanBrowserNewWindowPayload>(
    "human-browser-new-window",
    (event) => {
      const url = event.payload.url?.trim();
      if (!url || url === "about:blank") return;
      if (!shouldOpenNewWindowTab(url)) return;
      void humanBrowser.openTab(url);
    },
  );

  return () => {
    void unlistenNav.then((fn) => fn());
    void unlistenLoading.then((fn) => fn());
    void unlistenNavState.then((fn) => fn());
    void unlistenNewWindow.then((fn) => fn());
  };
}
