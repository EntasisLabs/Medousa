/** Cross-component Work hub chrome actions (rail toolbar → panel). */

import { layout } from "$lib/stores/layout.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { switchMobileTab } from "$lib/mobileNavigation";
import type { WorkHubLayer } from "$lib/utils/workHub";

export const WORK_FOCUS_ASK_EVENT = "medousa-work-focus-ask";
/** @deprecated Prefer setWorkRailFilter — kept for any lingering listeners. */
export const WORK_OPEN_TRAY_EVENT = "medousa-work-open-tray";

export type WorkTrayId = Exclude<WorkHubLayer, "living">;

export function dispatchWorkFocusAsk() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(WORK_FOCUS_ASK_EVENT));
}

/** Set the Work side-rail filter (living/settled/failed/…) and surface Work. */
export function dispatchWorkOpenTray(tray: WorkTrayId) {
  workspace.setWorkRailFilter(tray);
  if (layout.isMobile) {
    switchMobileTab("home");
  } else {
    shellTabs.openSurface("work", { activate: true });
  }
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent(WORK_OPEN_TRAY_EVENT, { detail: { tray } }),
  );
}

/** Open Work → Asks (desktop surface or mobile Home tab). */
export function openWorkAsks() {
  workspace.openAsksView();
  if (layout.isMobile) {
    switchMobileTab("home");
    return;
  }
  shellTabs.openSurface("work", { activate: true });
}
