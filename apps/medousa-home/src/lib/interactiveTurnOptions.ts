import type { InteractiveTurnOptions } from "$lib/daemon";
import { homeChannelSurface, isTauriMobilePlatform } from "$lib/platform";
import { runtime } from "$lib/stores/runtime.svelte";

/** Options for POST /v1/interactive/turn. */
export function buildInteractiveTurnOptions(): InteractiveTurnOptions {
  const channelSurface = homeChannelSurface();
  const shared = {
    responseDepthMode: runtime.depthMode,
    reasoningEffort: runtime.reasoningEffort,
    channelSurface,
  };

  // Companion shells load runtime from the workshop daemon — pass explicit routing so
  // composer model picks match turn requests once defaults are hydrated.
  if (isTauriMobilePlatform() && !runtime.defaultsLoaded) {
    return shared;
  }

  return {
    ...shared,
    provider: runtime.provider,
    model: runtime.model,
    stageRouting: runtime.stageRouting,
  };
}
