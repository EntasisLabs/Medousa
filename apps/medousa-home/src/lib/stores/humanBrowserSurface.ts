/** Resolve which human browser store owns the current window. */

import { humanBrowserEmbed, humanBrowserPopout } from "$lib/stores/humanBrowser.svelte";
import type { HumanBrowserStore } from "$lib/stores/humanBrowser.svelte";

export type HumanBrowserSurface = "embed" | "popout";

export function isPopoutBrowserChrome(): boolean {
  return (
    typeof window !== "undefined" &&
    window.location.pathname.includes("/popout/browser-chrome")
  );
}

export function humanBrowserForWindow(): HumanBrowserStore {
  return isPopoutBrowserChrome() ? humanBrowserPopout : humanBrowserEmbed;
}

export function humanBrowserSurfaceForWindow(): HumanBrowserSurface {
  return isPopoutBrowserChrome() ? "popout" : "embed";
}
