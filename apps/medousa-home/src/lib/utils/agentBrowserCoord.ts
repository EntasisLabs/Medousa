/** Global fan-out for human webview navigation → agent metadata stores. */

import { isTauri } from "$lib/platform";
import { humanBrowserEmbed } from "$lib/stores/humanBrowser.svelte";
import { browser } from "$lib/stores/browser.svelte";
import { browserContext } from "$lib/stores/browserContext.svelte";
import { attachHumanBrowserSurface } from "$lib/utils/humanBrowserListeners";

let agentNavigationInFlight = false;

/** Call before agent-initiated humanBrowser.navigate to avoid flipping control to user. */
export function markAgentNavigation() {
  agentNavigationInFlight = true;
}

export function attachAgentBrowserCoord(): () => void {
  if (!isTauri()) return () => {};

  const stopEmbedListeners = attachHumanBrowserSurface(humanBrowserEmbed, "embed");

  const unlistenNavForAgent = import("@tauri-apps/api/event").then(({ listen }) =>
    listen<import("$lib/humanBrowser").HumanBrowserNavigatedPayload>(
      "human-browser-navigated",
      (event) => {
        if ((event.payload.surface ?? "embed") !== "embed") return;
        browserContext.applyPayload(event.payload);
        void browser.syncFromNative(event.payload.url);

        if (agentNavigationInFlight) {
          agentNavigationInFlight = false;
          return;
        }
        if (browser.control === "agent") {
          void browser.setControl("user");
        }
      },
    ),
  );

  return () => {
    stopEmbedListeners();
    void unlistenNavForAgent.then((fn) => fn());
  };
}

export type { HumanBrowserNewWindowPayload } from "$lib/utils/humanBrowserListeners";
