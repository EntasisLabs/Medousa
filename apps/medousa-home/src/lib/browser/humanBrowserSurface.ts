/** Resolve which human browser store owns the current window. */

import { humanBrowserEmbed, humanBrowserPopout } from "$lib/stores/humanBrowser.svelte";
import type { HumanBrowserStore } from "$lib/stores/humanBrowser.svelte";
import {
  isPopoutBrowserChrome,
  type HumanBrowserSurface,
} from "$lib/utils/humanBrowserWindow";

export type { HumanBrowserSurface };
export { isPopoutBrowserChrome };

export function humanBrowserForWindow(): HumanBrowserStore {
  return isPopoutBrowserChrome() ? humanBrowserPopout : humanBrowserEmbed;
}

export function humanBrowserSurfaceForWindow(): HumanBrowserSurface {
  return isPopoutBrowserChrome() ? "popout" : "embed";
}
