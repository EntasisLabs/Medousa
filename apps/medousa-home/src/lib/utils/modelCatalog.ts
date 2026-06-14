import { modelPickKey } from "$lib/utils/formatModelDisplay";

export interface ModelPick {
  provider: string;
  model: string;
  label: string;
  hint?: string;
}

export interface FavoriteModel {
  provider: string;
  model: string;
}

export const MAX_FAVORITE_MODELS = 8;

/** Default model when switching to a provider (frontier-aware, June 2026). */
export const PROVIDER_DEFAULT_MODELS: Record<string, string> = {
  openai: "gpt-5.4-mini",
  anthropic: "claude-sonnet-4-6",
  google: "gemini-3.1-pro-preview",
  gemini: "gemini-3.1-pro-preview",
  deepseek: "deepseek-v4-flash",
  groq: "openai/gpt-oss-120b",
  xai: "grok-4-1-fast",
  ollama: "llama3.2",
  "medousa-local": "gemma-4-12b-it",
  mistral: "mistral-large-latest",
  openrouter: "openai/gpt-5.4-mini",
  together: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
  fireworks: "accounts/fireworks/models/llama-v3p3-70b-instruct",
  perplexity: "sonar-pro",
  cohere: "command-a-03-2025",
  "azure-openai": "gpt-5.4-mini",
  cerebras: "llama-3.3-70b",
  hyperbolic: "meta-llama/Meta-Llama-3.3-70B-Instruct",
  huggingface: "meta-llama/Meta-Llama-3.3-70B-Instruct",
  replicate: "meta/meta-llama-3-8b-instruct",
  moonshot: "kimi-k2-0711-preview",
  qwen: "qwen-plus",
  zhipu: "glm-4-plus",
  minimax: "MiniMax-Text-01",
  bedrock: "anthropic.claude-sonnet-4-6",
};

/** Curated frontier picks shown in composer and settings. */
export const CURATED_MODEL_PICKS: ModelPick[] = [
  { provider: "openai", model: "gpt-5.4", label: "GPT-5.4", hint: "OpenAI flagship" },
  { provider: "openai", model: "gpt-5.4-mini", label: "GPT-5.4 Mini", hint: "Fast & efficient" },
  { provider: "anthropic", model: "claude-sonnet-4-6", label: "Sonnet 4.6", hint: "Best balance" },
  { provider: "anthropic", model: "claude-opus-4-8", label: "Opus 4.8", hint: "Maximum depth" },
  {
    provider: "google",
    model: "gemini-3.1-pro-preview",
    label: "Gemini 3.1 Pro",
    hint: "Google flagship",
  },
  {
    provider: "google",
    model: "gemini-3-flash-preview",
    label: "Gemini 3 Flash",
    hint: "Fast multimodal",
  },
  {
    provider: "deepseek",
    model: "deepseek-v4-flash",
    label: "DeepSeek V4 Flash",
    hint: "Fast & cheap",
  },
  {
    provider: "deepseek",
    model: "deepseek-v4-pro",
    label: "DeepSeek V4 Pro",
    hint: "DeepSeek flagship",
  },
  {
    provider: "groq",
    model: "openai/gpt-oss-120b",
    label: "GPT-OSS 120B",
    hint: "Groq hosted",
  },
  {
    provider: "groq",
    model: "llama-3.3-70b-versatile",
    label: "Llama 3.3 70B",
    hint: "Groq fast",
  },
  { provider: "xai", model: "grok-4-1-fast", label: "Grok 4.1 Fast", hint: "xAI fast" },
  { provider: "xai", model: "grok-4", label: "Grok 4", hint: "xAI flagship" },
];

export function defaultProviderModel(providerId: string): string | undefined {
  return PROVIDER_DEFAULT_MODELS[providerId.trim().toLowerCase()];
}

export function curatedPicksForProvider(providerId: string): ModelPick[] {
  const id = providerId.trim().toLowerCase();
  return CURATED_MODEL_PICKS.filter((pick) => pick.provider === id);
}

export function favoriteKey(provider: string, model: string): string {
  return modelPickKey(provider, model);
}

export function normalizeFavoriteModels(raw: unknown): FavoriteModel[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const out: FavoriteModel[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const provider = String((entry as FavoriteModel).provider ?? "").trim();
    const model = String((entry as FavoriteModel).model ?? "").trim();
    if (!provider || !model) continue;
    const key = favoriteKey(provider, model);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ provider, model });
    if (out.length >= MAX_FAVORITE_MODELS) break;
  }
  return out;
}

export function isFavoriteModel(
  favorites: FavoriteModel[],
  provider: string,
  model: string,
): boolean {
  const key = favoriteKey(provider, model);
  return favorites.some((entry) => favoriteKey(entry.provider, entry.model) === key);
}

export function toggleFavoriteModel(
  favorites: FavoriteModel[],
  provider: string,
  model: string,
): FavoriteModel[] {
  const key = favoriteKey(provider, model);
  if (isFavoriteModel(favorites, provider, model)) {
    return favorites.filter((entry) => favoriteKey(entry.provider, entry.model) !== key);
  }
  const next = [{ provider: provider.trim(), model: model.trim() }, ...favorites];
  return next.slice(0, MAX_FAVORITE_MODELS);
}

export function favoriteToPick(entry: FavoriteModel): ModelPick {
  const curated = CURATED_MODEL_PICKS.find(
    (pick) => favoriteKey(pick.provider, pick.model) === favoriteKey(entry.provider, entry.model),
  );
  return {
    provider: entry.provider,
    model: entry.model,
    label: curated?.label ?? entry.model,
    hint: curated?.hint ?? "Favorite",
  };
}
