import type { ShellTab } from "$lib/types/shellTabs";
import type { LmeTab } from "$lib/stores/lmeWorkspace.svelte";

export function focusSurfaceHint(
  tab: ShellTab | null,
  lmeTabs: LmeTab[],
): string | null {
  if (!tab) return null;
  if (tab.kind === "create") return "create";
  if (tab.kind === "chat") return "chat";
  if (tab.kind === "lme") {
    // Prefer the open tab’s family — explorerMode is intentionally not synced on activate.
    // Inline map (avoid importing lmeExplorerModes → circular init with lmeWorkspace).
    const lme = lmeTabs.find((entry) => entry.tabId === tab.lmeTabId);
    switch (lme?.kind) {
      case "manuscript":
        return "workshop";
      case "script":
      case "flow":
      case "schedule":
        return "automations";
      case "note":
      case "file":
      case "deck":
        return "library";
      case "code":
        return lme.resource.kind === "review" ||
          lme.resource.kind === "hosted_review"
          ? "reviews"
          : "code";
      default:
        return "library";
    }
  }
  if (tab.kind === "web") return "web";
  if (tab.kind === "terminal") return null;
  return tab.surfaceId;
}
