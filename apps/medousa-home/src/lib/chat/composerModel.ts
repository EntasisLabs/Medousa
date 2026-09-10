import { compatibleReasoning } from "$lib/types/reasoningEffort";
import { reasoningCapabilities } from "./reasoningCapabilities.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { defaultStageRouting } from "$lib/utils/stageRouting";
import { sessionModelSelections, type SessionScope } from "./sessionModelSelection.svelte";

/** Pure read for reactive composer labels and capability hints. */
export function composerModel() {
  return sessionModelSelections.resolve(chat, runtime)!;
}

export function composerSessionScope(): SessionScope {
  return { sessionId: chat.sessionId, workshopScopeId: chat.workshopScopeId };
}

export function selectComposerModel(provider: string, model: string, scope = composerSessionScope()) {
  const nextProvider = provider.trim();
  const nextModel = model.trim();
  if (!nextProvider || !nextModel) return;
  sessionModelSelections.set(scope, {
    provider: nextProvider,
    model: nextModel,
    stageRouting: defaultStageRouting(nextProvider, nextModel),
  });
}

/** Reasoning is remembered independently for each provider/model in this chat. */
export function composerReasoning() {
  const model = composerModel();
  const capability = reasoningCapabilities.get(chat.workshopScopeId, model.provider, model.model);
  const requested = model.reasoningByModel?.[JSON.stringify([model.provider, model.model])] ?? "default";
  const value = compatibleReasoning(requested, capability);
  return { capability, value, adjusted: value !== requested };
}

export function selectComposerReasoning(value: string) {
  const selection = composerModel();
  const capability = reasoningCapabilities.get(chat.workshopScopeId, selection.provider, selection.model);
  sessionModelSelections.setReasoning(chat, selection, compatibleReasoning(value, capability));
}
