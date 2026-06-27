/** Keep human browser session aligned when switching desktop ↔ mobile shells. */

import { humanBrowserEmbedHide } from "$lib/humanBrowser";
import { layout } from "$lib/stores/layout.svelte";
import { isTauri } from "$lib/platform";

let handoffTimer: ReturnType<typeof setTimeout> | null = null;

export function handoffBrowserShell(toMobile: boolean) {
  if (!isTauri()) return;

  if (handoffTimer) clearTimeout(handoffTimer);
  handoffTimer = setTimeout(() => {
    handoffTimer = null;
    void humanBrowserEmbedHide();

    if (toMobile) {
      if (layout.desktopSurface === "web") {
        layout.openWeb();
      }
      return;
    }

    if (layout.mobileTab === "web") {
      layout.navigateDesktop("web");
    }
  }, 150);
}
