import type { MobileAskEntry } from "$lib/types/environment";
import { MOBILE_TABS, type MobileTab } from "$lib/types/mobile";

/**
 * Primary mobile destinations (destinations menu). Always reachable —
 * layout presets / legacy `tabBar: "minimal"` used to hide Notes/Web from the
 * retired bottom tab bar, which bounced those destinations back to Home.
 */
const PRIMARY_MOBILE_TABS = new Set<MobileTab>([
  "home",
  "chat",
  "notes",
  "web",
  "more",
]);

/** Mobile tabs reachable from the destinations menu / swipe order. */
export function visibleMobileTabs(_spec?: unknown): MobileTab[] {
  return MOBILE_TABS.map((tab) => tab.id).filter((tab) =>
    PRIMARY_MOBILE_TABS.has(tab),
  );
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
