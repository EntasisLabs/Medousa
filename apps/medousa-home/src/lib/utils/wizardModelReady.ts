import { getRuntimeDefaults } from "$lib/daemon";
import type { ProvidersProbeResult } from "$lib/utils/providersApi";
import { applyWizardScreen1 } from "$lib/utils/providersApi";
import { isTauri } from "$lib/window";

export function hasWizardModelConfig(
  existingProvider?: string | null,
  existingModel?: string | null,
): boolean {
  return Boolean(existingProvider?.trim() && existingModel?.trim());
}

export async function runtimeHasModelConfig(): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    const defaults = await getRuntimeDefaults();
    return Boolean(defaults.provider?.trim() && defaults.model?.trim());
  } catch {
    return false;
  }
}

/** Ensure a chat-ready provider/model exists before skipping wizard setup. */
export async function ensureSkipReadyModel(
  existingProvider: string | null | undefined,
  existingModel: string | null | undefined,
  probe: ProvidersProbeResult | null,
): Promise<{ ok: true } | { ok: false; message: string }> {
  if (hasWizardModelConfig(existingProvider, existingModel)) {
    return { ok: true };
  }
  if (await runtimeHasModelConfig()) {
    return { ok: true };
  }

  if (probe?.ollamaDetected) {
    const model =
      probe.suggestedOllamaModel?.trim() ||
      probe.ollamaModels[0]?.trim() ||
      "llama3.2";
    await applyWizardScreen1({
      path: "byok",
      provider: "ollama",
      model,
      baseUrl: probe.ollamaBaseUrl || null,
      startCore: false,
    });
    return { ok: true };
  }

  return {
    ok: false,
    message:
      "Pick a brain above first — use Offline Gemma, add an API key, or install Ollama for a free local model.",
  };
}
