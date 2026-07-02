import type { EnvironmentSpec, MobileAskEntry } from "$lib/types/environment";
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import { MOBILE_TABS, type MobileTab } from "$lib/types/mobile";

const TAB_REQUIRED_SURFACE: Record<Exclude<MobileTab, "more" | "home">, string> = {
  chat: "chat",
  notes: "library",
  web: "web",
};

function activePresetSurfaceIds(spec: EnvironmentSpec): Set<string> {
  const preset = spec.layoutPresets?.find((entry) => entry.active);
  const ids = preset?.surfaces ?? spec.surfaces.map((surface) => surface.id);
  return new Set(ids);
}

/** Mobile tabs visible for the active preset and shellChrome.mobile.tabBar mode. */
export function visibleMobileTabs(spec?: EnvironmentSpec | null): MobileTab[] {
  const resolved = spec ?? defaultEnvironmentSpec();
  const presetIds = activePresetSurfaceIds(resolved);
  const tabBar = resolved.shellChrome?.mobile?.tabBar ?? "full";

  return MOBILE_TABS.map((tab) => tab.id).filter((tab) => {
    if (tab === "more") return true;
    if (tabBar === "minimal" && (tab === "notes" || tab === "web")) {
      return false;
    }
    if (tab === "home") return true;
    const required = TAB_REQUIRED_SURFACE[tab as keyof typeof TAB_REQUIRED_SURFACE];
    return presetIds.has(required);
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
