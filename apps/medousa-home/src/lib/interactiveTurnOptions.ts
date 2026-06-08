import type { InteractiveTurnOptions } from "$lib/daemon";
import { homeChannelSurface, isTauriMobilePlatform } from "$lib/platform";
import { runtime } from "$lib/stores/runtime.svelte";

/** Options for POST /v1/interactive/turn. Mobile omits model routing so the Mac daemon uses tui_defaults. */
export function buildInteractiveTurnOptions(): InteractiveTurnOptions {
  const channelSurface = homeChannelSurface();
  if (isTauriMobilePlatform()) {
    return { responseDepthMode: runtime.depthMode, channelSurface };
  }

  return {
    provider: runtime.provider,
    model: runtime.model,
    responseDepthMode: runtime.depthMode,
    stageRouting: runtime.stageRouting,
    channelSurface,
  };
}
