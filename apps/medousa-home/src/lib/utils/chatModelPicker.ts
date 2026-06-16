import type { ProvidersListResult } from "$lib/types/providers";
import type { ProvidersProbeResult } from "$lib/utils/providersApi";
import { modelPickKey } from "$lib/utils/formatModelDisplay";
import type { FavoriteModel } from "$lib/utils/modelCatalog";
import {
  CURATED_MODEL_PICKS,
  favoriteToPick,
  resolveModelDisplayLabel,
} from "$lib/utils/modelCatalog";

export interface ChatModelPickOption {
  key: string;
  provider: string;
  model: string;
  label: string;
  hint?: string;
  favorite?: boolean;
}

export function buildChatModelOptions(
  catalog: ProvidersListResult,
  probe: ProvidersProbeResult | null,
  currentProvider: string,
  currentModel: string,
  favorites: FavoriteModel[] = [],
): ChatModelPickOption[] {
  const options: ChatModelPickOption[] = [];
  const seen = new Set<string>();

  const push = (
    provider: string,
    model: string,
    label: string,
    hint?: string,
    favorite = false,
  ) => {
    const key = modelPickKey(provider, model);
    if (seen.has(key)) return;
    seen.add(key);
    options.push({ key, provider, model, label, hint, favorite });
  };

  for (const entry of favorites) {
    const pick = favoriteToPick(entry);
    push(pick.provider, pick.model, pick.label, pick.hint ?? "Favorite", true);
  }

  const provider = currentProvider.trim();
  const model = currentModel.trim();
  if (provider && model) {
    push(provider, model, resolveModelDisplayLabel(provider, model), "Active");
  }

  for (const pick of CURATED_MODEL_PICKS) {
    push(pick.provider, pick.model, pick.label, pick.hint);
  }

  for (const entry of catalog.providers) {
    if (entry.category === "local" || entry.id === "ollama") {
      const defaultModel = entry.defaultModel;
      if (!CURATED_MODEL_PICKS.some((pick) => pick.provider === entry.id && pick.model === defaultModel)) {
        push(entry.id, defaultModel, defaultModel, entry.label);
      }
    }
  }

  if (probe?.ollamaModels?.length) {
    for (const ollamaModel of probe.ollamaModels) {
      push("ollama", ollamaModel, ollamaModel, "Ollama");
    }
  }

  return options;
}

/** Favorites, curated picks, and the active model — for mobile composer dropdowns. */
export function buildMobileModelDropdownOptions(
  catalog: ProvidersListResult,
  probe: ProvidersProbeResult | null,
  currentProvider: string,
  currentModel: string,
  favorites: FavoriteModel[] = [],
): ChatModelPickOption[] {
  const options = buildChatModelOptions(
    catalog,
    probe,
    currentProvider,
    currentModel,
    favorites,
  );
  const curatedKeys = new Set(
    CURATED_MODEL_PICKS.map((pick) => modelPickKey(pick.provider, pick.model)),
  );
  const activeKey = modelPickKey(currentProvider, currentModel);

  return options.filter(
    (option) => option.favorite || curatedKeys.has(option.key) || option.key === activeKey,
  );
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

/** Two-letter monogram for composer model badge. */
export function providerMonogram(provider: string): string {
  const id = provider.trim().toLowerCase();
  if (id === "openai") return "OA";
  if (id === "anthropic") return "AN";
  if (id === "ollama") return "OL";
  if (id === "groq") return "GQ";
  if (id === "deepseek") return "DS";
  if (id === "google") return "GG";
  if (id.length >= 2) return id.slice(0, 2).toUpperCase();
  return id.toUpperCase() || "AI";
}

export function depthModeLabel(mode: string): string {
  if (mode === "concise") return "Concise";
  if (mode === "deep") return "Deep";
  return "Standard";
}
