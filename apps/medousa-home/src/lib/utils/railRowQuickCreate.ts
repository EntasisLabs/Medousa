/**
 * Primary "+" quick create for master-rail rows.
 * Floating toolbars stay on shake / keybind only.
 */
import { calendar } from "$lib/stores/calendar.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { peersShell } from "$lib/stores/peersShell.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { dispatchBrowserFocusUrl } from "$lib/utils/browserChromeEvents";
import { dispatchWorkFocusAsk } from "$lib/utils/workChromeEvents";
import { SAFETY_SURFACE_SETTINGS } from "$lib/types/environment";

export type RailQuickCreateResult = {
  /** Surface to activate in main content after the create action (if any). */
  navigateTo?: string;
};

/**
 * Settings, Context, Profiles (YouCreateMenu), and custom surfaces
 * have no direct row “+” create action.
 */
export function surfaceShowsRailQuickCreate(
  surfaceId: string,
  kind?: string | null,
): boolean {
  if (kind === "custom") return false;
  if (
    surfaceId === SAFETY_SURFACE_SETTINGS ||
    surfaceId === "settings" ||
    surfaceId === "context" ||
    surfaceId === "profiles"
  ) {
    return false;
  }
  return true;
}

export async function runRailRowQuickCreate(
  surfaceId: string,
): Promise<RailQuickCreateResult> {
  switch (surfaceId) {
    case "chat":
      await chat.newSession();
      return { navigateTo: "chat" };
    case "peers":
      peersShell.requestAddPeer();
      return { navigateTo: "peers" };
    case "web":
      await humanBrowser.openTab("about:blank");
      dispatchBrowserFocusUrl();
      return { navigateTo: "web" };
    case "calendar":
      calendar.openCreate();
      return { navigateTo: "calendar" };
    case "work":
      workspace.openHubView();
      dispatchWorkFocusAsk();
      return { navigateTo: "work" };
    case "library":
      lmeWorkspace.setExplorerMode("notes");
      vault.openNewNoteDialog();
      return { navigateTo: "library" };
    case "automations":
      // Opens a real script editor tab (not an empty automations surface).
      lmeWorkspace.openNewScript();
      return {};
    default:
      return { navigateTo: surfaceId };
  }
}

export function railRowQuickCreateLabel(surfaceId: string): string {
  switch (surfaceId) {
    case "chat":
      return "New chat";
    case "peers":
      return "Add peer";
    case "web":
      return "New tab";
    case "calendar":
      return "New event";
    case "work":
      return "New ask";
    case "library":
      return "New note";
    case "automations":
      return "New script";
    default:
      return "Quick create";
  }
}
