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
  badges?: string[];
}

export interface ChatModelPickGroup {
  provider: string;
  label: string;
  options: ChatModelPickOption[];
}

const PROVIDER_LABELS: Record<string, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  google: "Google",
  deepseek: "DeepSeek",
  groq: "Groq",
  xai: "xAI",
  ollama: "Ollama",
  openrouter: "OpenRouter",
  "medousa-local": "Medousa Local",
};

export function resolveProviderLabel(
  catalog: ProvidersListResult | null,
  providerId: string,
): string {
  const id = providerId.trim().toLowerCase();
  const fromCatalog = catalog?.providers.find((entry) => entry.id.toLowerCase() === id);
  if (fromCatalog) return fromCatalog.label;
  return PROVIDER_LABELS[id] ?? (id ? id.charAt(0).toUpperCase() + id.slice(1) : "Provider");
}

export function groupChatModelOptions(
  options: ChatModelPickOption[],
  catalog: ProvidersListResult | null,
  activeProvider?: string,
): ChatModelPickGroup[] {
  const byProvider = new Map<string, ChatModelPickOption[]>();
  for (const option of options) {
    const provider = option.provider.trim().toLowerCase();
    const list = byProvider.get(provider) ?? [];
    list.push(option);
    byProvider.set(provider, list);
  }
  const active = activeProvider?.trim().toLowerCase() ?? "";
  return Array.from(byProvider.entries())
    .map(([provider, groupOptions]) => ({
      provider,
      label: resolveProviderLabel(catalog, provider),
      options: groupOptions,
    }))
    .sort((left, right) => {
      if (active) {
        if (left.provider === active && right.provider !== active) return -1;
        if (right.provider === active && left.provider !== active) return 1;
      }
      return left.label.localeCompare(right.label);
    });
}

export function groupNonFavoriteChatModelOptions(
  options: ChatModelPickOption[],
  catalog: ProvidersListResult | null,
  activeProvider?: string,
): ChatModelPickGroup[] {
  return groupChatModelOptions(
    options.filter((option) => !option.favorite),
    catalog,
    activeProvider,
  );
}

export function mergeLiveProviderModels(
  options: ChatModelPickOption[],
  providerId: string,
  liveModels: string[],
  catalog: ProvidersListResult | null,
): ChatModelPickOption[] {
  if (!liveModels.length) return options;
  const provider = providerId.trim().toLowerCase();
  const seen = new Set(options.map((option) => option.key));
  const next = [...options];
  for (const model of liveModels) {
    const trimmed = model.trim();
    if (!trimmed) continue;
    const key = modelPickKey(provider, trimmed);
    if (seen.has(key)) continue;
    seen.add(key);
    next.push({
      key,
      provider,
      model: trimmed,
      label: resolveModelDisplayLabel(provider, trimmed),
      hint: resolveProviderLabel(catalog, provider),
    });
  }
  return next;
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
      (option.hint?.toLowerCase().includes(needle) ?? false) ||
      (option.badges?.some((badge) => badge.toLowerCase().includes(needle)) ?? false),
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
