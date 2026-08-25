import type { EnvironmentSpec, MobileAskEntry } from "$lib/types/environment";
import { MOBILE_TABS, type MobileTab } from "$lib/types/mobile";
import { activePresetSurfaceIds } from "$lib/utils/environmentLayout";

const MOBILE_TAB_SURFACE_IDS: Partial<Record<MobileTab, string>> = {
  chat: "chat",
  notes: "notes",
  web: "web",
};

/** Mobile swipe/chrome order projected from the shared active layout preset. */
export function visibleMobileTabs(spec?: EnvironmentSpec | null): MobileTab[] {
  if (!spec) return MOBILE_TABS.map((tab) => tab.id);
  const visibleIds = new Set(activePresetSurfaceIds(spec));
  return MOBILE_TABS.map((tab) => tab.id).filter((tab) => {
    // Home is the safe landing; More is the doorway to pinned utilities.
    if (tab === "home" || tab === "more") return true;
    const surfaceId = MOBILE_TAB_SURFACE_IDS[tab];
    return Boolean(surfaceId && visibleIds.has(surfaceId));
  });
}

export function showBuiltinHomeInlineAsk(askEntry: MobileAskEntry | null | undefined): boolean {
  return (askEntry ?? "inline") === "inline";
}

/** Shell-level FAB when askEntry=fab and no chrome_action fab on custom home. */
export function shellAskFabVisible(options: {
  askEntry: MobileAskEntry | null | undefined;
  customHome: boolean;
  fabChromeActionCount: number;
}): boolean {
  const entry = options.askEntry ?? "inline";
  if (entry === "tab_only") return false;
  if (entry !== "fab") return false;
  if (!options.customHome) return true;
  return options.fabChromeActionCount === 0;
}
