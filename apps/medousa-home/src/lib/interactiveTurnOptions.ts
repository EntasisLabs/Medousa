import { sessionModelSelections, type ChatModelContext } from "$lib/chat/sessionModelSelection.svelte";
import type { InteractiveTurnOptions } from "$lib/daemon";
import { homeChannelSurface, isTauriMobilePlatform } from "$lib/platform";
import { governedBrowser } from "$lib/stores/governedBrowser.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { userProfiles } from "$lib/stores/userProfiles.svelte";

/** Options for POST /v1/interactive/turn. */
export function buildInteractiveTurnOptions(context: ChatModelContext): InteractiveTurnOptions {
  const channelSurface = homeChannelSurface();
  const shared = {
    responseDepthMode: runtime.depthMode,
    reasoningEffort: runtime.reasoningEffort,
    channelSurface,
    browserDriverId: governedBrowser.turnBrowserDriverId ?? undefined,
    selectedWorlds: governedBrowser.turnWorldSelection
      ? [governedBrowser.turnWorldSelection]
      : undefined,
    identityUserId: userProfiles.turnIdentityUserId(),
  };

  // Before mobile defaults arrive, defer to the daemon unless this chat already
  // has a selection or model receipt. Never persist the mobile placeholder.
  const defaults = isTauriMobilePlatform() && !runtime.defaultsLoaded ? null : runtime;
  const selection = sessionModelSelections.resolve(context, defaults);
  if (!selection) return shared;
  sessionModelSelections.set(context, selection);
  return { ...shared, ...selection };
}
