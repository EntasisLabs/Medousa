/** Global fan-out for human webview navigation → agent metadata stores. */

import { listen } from "@tauri-apps/api/event";
import { isTauri } from "$lib/platform";
import type { HumanBrowserNavigatedPayload } from "$lib/humanBrowser";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { browser } from "$lib/stores/browser.svelte";
import { browserContext } from "$lib/stores/browserContext.svelte";

let agentNavigationInFlight = false;

/** Call before agent-initiated humanBrowser.navigate to avoid flipping control to user. */
export function markAgentNavigation() {
  agentNavigationInFlight = true;
}

export function attachAgentBrowserCoord(): () => void {
  if (!isTauri()) return () => {};

  const unlisten = listen<HumanBrowserNavigatedPayload>("human-browser-navigated", (event) => {
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

  return () => {
    void unlisten.then((fn) => fn());
  };
}
