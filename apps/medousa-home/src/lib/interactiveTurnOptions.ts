import { compatibleReasoning } from "$lib/types/reasoningEffort";
import { reasoningCapabilities } from "$lib/chat/reasoningCapabilities.svelte";
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
    reasoningEffort: "default",
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
  const capability = reasoningCapabilities.get(context.workshopScopeId, selection.provider, selection.model);
  const reasoningEffort = compatibleReasoning(
    sessionModelSelections.reasoning(context, selection.provider, selection.model), capability);
  return { ...shared, provider: selection.provider, model: selection.model, stageRouting: selection.stageRouting, reasoningEffort };
}

/** Freeze the turn's route before awaiting capability discovery. */
export async function prepareInteractiveTurnOptions(context: ChatModelContext): Promise<InteractiveTurnOptions> {
  const options = buildInteractiveTurnOptions(context);
  if (!options.provider || !options.model) return options;
  const scope = { sessionId: context.sessionId, workshopScopeId: context.workshopScopeId };
  const requested = sessionModelSelections.reasoning(scope, options.provider, options.model);
  await reasoningCapabilities.load(scope.workshopScopeId, options.provider, options.model);
  if (context.sessionId !== scope.sessionId || context.workshopScopeId !== scope.workshopScopeId) {
    throw new Error("Conversation changed while preparing the turn. Send again in the selected chat.");
  }
  return { ...options, reasoningEffort: compatibleReasoning(requested,
    reasoningCapabilities.get(scope.workshopScopeId, options.provider, options.model)) };
}
