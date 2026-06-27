/** Keep human browser session aligned when switching desktop ↔ mobile shells. */

import { humanBrowserSetMobileShellActive } from "$lib/humanBrowser";
import { layout } from "$lib/stores/layout.svelte";
import { isTauri } from "$lib/platform";

let handoffTimer: ReturnType<typeof setTimeout> | null = null;

export function handoffBrowserShell(toMobile: boolean) {
  if (!isTauri()) return;

  void humanBrowserSetMobileShellActive(toMobile);

  if (handoffTimer) clearTimeout(handoffTimer);
  handoffTimer = null;

  if (toMobile) {
    handoffTimer = setTimeout(() => {
      handoffTimer = null;
      if (layout.desktopSurface === "web") {
        layout.openWeb();
      }
    }, 150);
    return;
  }

  // Desktop: navigate immediately so HumanBrowserPanel mounts before embed hide races.
  if (layout.mobileTab === "web") {
    layout.navigateDesktop("web");
  }
}
