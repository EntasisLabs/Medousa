import type { ProvidersListResult } from "$lib/types/providers";
import type { ProvidersProbeResult } from "$lib/utils/providersApi";
import { modelPickKey } from "$lib/utils/formatModelDisplay";

export interface ChatModelPickOption {
  key: string;
  provider: string;
  model: string;
  label: string;
  hint?: string;
}

export function buildChatModelOptions(
  catalog: ProvidersListResult,
  probe: ProvidersProbeResult | null,
  currentProvider: string,
  currentModel: string,
): ChatModelPickOption[] {
  const options: ChatModelPickOption[] = [];
  const seen = new Set<string>();

  const push = (provider: string, model: string, label: string, hint?: string) => {
    const key = modelPickKey(provider, model);
    if (seen.has(key)) return;
    seen.add(key);
    options.push({ key, provider, model, label, hint });
  };

  const provider = currentProvider.trim();
  const model = currentModel.trim();
  if (provider && model) {
    push(provider, model, model, "Active");
  }

  for (const entry of catalog.providers) {
    if (entry.category === "featured" || entry.id === "openai" || entry.id === "anthropic") {
      push(entry.id, entry.defaultModel, entry.defaultModel, entry.label);
    }
  }

  for (const entry of catalog.providers) {
    if (entry.category === "local" || entry.id === "ollama") {
      push(entry.id, entry.defaultModel, entry.defaultModel, entry.label);
    }
  }

  if (probe?.ollamaModels?.length) {
    for (const ollamaModel of probe.ollamaModels) {
      push("ollama", ollamaModel, ollamaModel, "Ollama");
    }
  }

  return options;
}

export function filterChatModelOptions(
  options: ChatModelPickOption[],
  query: string,
): ChatModelPickOption[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return options;
  return options.filter(
    (option) =>
      option.label.toLowerCase().includes(needle) ||
      option.model.toLowerCase().includes(needle) ||
      option.provider.toLowerCase().includes(needle) ||
      (option.hint?.toLowerCase().includes(needle) ?? false),
  );
}
