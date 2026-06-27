/** Read-only browser context for the main workshop window (ActivityPanel). */

import { listen } from "@tauri-apps/api/event";
import { isTauri } from "$lib/window";
import type { HumanBrowserNavigatedPayload } from "$lib/humanBrowser";

function tabLabelFromUrl(url: string): string {
  if (!url || url === "about:blank") return "Web";
  try {
    return new URL(url).hostname || url;
  } catch {
    return url.slice(0, 48);
  }
}

export class BrowserContextStore {
  activeUrl = $state("about:blank");
  scopeLabel = $state("Web");

  applyPayload(payload: HumanBrowserNavigatedPayload) {
    const url = payload.url?.trim() || "about:blank";
    this.activeUrl = url;
    this.scopeLabel = payload.title?.trim() || tabLabelFromUrl(url);
  }

  attachListeners(): () => void {
    if (!isTauri()) return () => {};

    const unlisteners: Promise<() => void>[] = [];
    unlisteners.push(
      listen<HumanBrowserNavigatedPayload>("human-browser-navigated", (event) => {
        this.applyPayload(event.payload);
      }),
    );

    return () => {
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }
}

export const browserContext = new BrowserContextStore();
