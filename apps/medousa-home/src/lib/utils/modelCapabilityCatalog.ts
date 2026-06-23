import { isTauri } from "$lib/window";
import type {
  Modality,
  ModelCapabilitiesLookupResponse,
  ModelCapabilityRecord,
  ModelCatalogListResponse,
  ModelCatalogRefreshResponse,
  ModelPricing,
} from "$lib/types/modelCapability";
import type { ProviderCatalogEntry } from "$lib/types/providers";
import { modelPickKey } from "$lib/utils/formatModelDisplay";
import { resolveModelDisplayLabel } from "$lib/utils/modelCatalog";

export interface ModelCatalogQuery {
  provider?: string;
  capability?: "vision" | "text";
  q?: string;
}

function requireTauri(): void {
  if (!isTauri()) {
    throw new Error("Model catalog requires the Medousa app.");
  }
}

function readString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function readOptionalString(value: unknown): string | null {
  const text = readString(value);
  return text || null;
}

function readOptionalNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function readOptionalBoolean(value: unknown): boolean | null {
  return typeof value === "boolean" ? value : null;
}

function normalizeModality(value: unknown): Modality {
  const text = readString(value).toLowerCase();
  if (text === "image") return "image";
  if (text === "audio") return "audio";
  if (text === "file") return "file";
  if (text === "video") return "video";
  return "text";
}

function normalizeModalities(value: unknown): Modality[] {
  if (!Array.isArray(value)) return ["text"];
  const modalities = value.map(normalizeModality);
  return modalities.length > 0 ? modalities : ["text"];
}

function normalizePricing(raw: unknown): ModelPricing | null {
  if (!raw || typeof raw !== "object") return null;
  const pricing = raw as Record<string, unknown>;
  const promptPerTokenUsd =
    readOptionalNumber(pricing.promptPerTokenUsd) ??
    readOptionalNumber(pricing.prompt_per_token_usd);
  const completionPerTokenUsd =
    readOptionalNumber(pricing.completionPerTokenUsd) ??
    readOptionalNumber(pricing.completion_per_token_usd);
  const imagePerUnitUsd =
    readOptionalNumber(pricing.imagePerUnitUsd) ??
    readOptionalNumber(pricing.image_per_unit_usd);
  if (
    promptPerTokenUsd == null &&
    completionPerTokenUsd == null &&
    imagePerUnitUsd == null
  ) {
    return null;
  }
  return {
    promptPerTokenUsd,
    completionPerTokenUsd,
    imagePerUnitUsd,
  };
}

/** Daemon registry records use snake_case; normalize for the TS surface. */
export function normalizeModelCapabilityRecord(raw: unknown): ModelCapabilityRecord | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  const provider = readString(record.provider);
  const modelId = readString(record.modelId) || readString(record.model_id);
  if (!provider || !modelId) return null;

  const inputModalities = normalizeModalities(
    record.inputModalities ?? record.input_modalities,
  );
  const outputModalities = normalizeModalities(
    record.outputModalities ?? record.output_modalities,
  );
  const supportsVisionRaw =
    record.supportsVision ?? record.supports_vision ?? inputModalities.includes("image");

  return {
    provider,
    modelId,
    displayName:
      readOptionalString(record.displayName) ?? readOptionalString(record.display_name),
    inputModalities,
    outputModalities,
    maxInputTokens:
      readOptionalNumber(record.maxInputTokens) ??
      readOptionalNumber(record.max_input_tokens),
    maxOutputTokens:
      readOptionalNumber(record.maxOutputTokens) ??
      readOptionalNumber(record.max_output_tokens),
    supportsToolCalling:
      readOptionalBoolean(record.supportsToolCalling) ??
      readOptionalBoolean(record.supports_tool_calling),
    supportsVision: Boolean(supportsVisionRaw),
    pricing: normalizePricing(record.pricing),
    source: readString(record.source) || "catalog",
    fetchedAt:
      readString(record.fetchedAt) ||
      readString(record.fetched_at) ||
      new Date().toISOString(),
  };
}

function normalizeCatalogListResponse(raw: unknown): ModelCatalogListResponse {
  const payload = (raw ?? {}) as Record<string, unknown>;
  const modelsRaw = Array.isArray(payload.models) ? payload.models : [];
  const models = modelsRaw
    .map(normalizeModelCapabilityRecord)
    .filter((record): record is ModelCapabilityRecord => record != null);
  return {
    freshness: (payload.freshness ?? {
      ttlSecs: 86_400,
      providers: [],
    }) as ModelCatalogListResponse["freshness"],
    models,
  };
}

function normalizeLookupResponse(raw: unknown): ModelCapabilitiesLookupResponse {
  const payload = (raw ?? {}) as Record<string, unknown>;
  const model = normalizeModelCapabilityRecord(payload.model);
  return {
    found: Boolean(payload.found),
    model,
    heuristic: Boolean(payload.heuristic),
  };
}

export function recordsFromModelIds(
  provider: string,
  modelIds: string[],
  source: string,
): ModelCapabilityRecord[] {
  const providerId = provider.trim();
  return modelIds
    .map((modelId) => modelId.trim())
    .filter(Boolean)
    .map((modelId) => ({
      provider: providerId,
      modelId,
      displayName: resolveModelDisplayLabel(providerId, modelId, 40),
      inputModalities: ["text"] as Modality[],
      outputModalities: ["text"] as Modality[],
      supportsVision: false,
      source,
      fetchedAt: new Date().toISOString(),
    }));
}

export function defaultProviderRecords(entry: ProviderCatalogEntry): ModelCapabilityRecord[] {
  const modelId = entry.defaultModel.trim();
  if (!modelId) return [];
  return recordsFromModelIds(entry.id, [modelId], "catalog.default");
}

export async function listModelCatalog(
  query: ModelCatalogQuery = {},
): Promise<ModelCatalogListResponse> {
  requireTauri();
  const { invoke } = await import("@tauri-apps/api/core");
  const raw = await invoke<unknown>("model_catalog_list", {
    provider: query.provider?.trim() || null,
    capability: query.capability?.trim() || null,
    q: query.q?.trim() || null,
  });
  return normalizeCatalogListResponse(raw);
}

export async function lookupModelCapabilities(
  provider: string,
  model: string,
): Promise<ModelCapabilitiesLookupResponse> {
  requireTauri();
  const { invoke } = await import("@tauri-apps/api/core");
  const raw = await invoke<unknown>("model_catalog_lookup", {
    provider: provider.trim(),
    model: model.trim(),
  });
  return normalizeLookupResponse(raw);
}

export async function refreshModelCatalog(
  providers?: string[],
): Promise<ModelCatalogRefreshResponse> {
  requireTauri();
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ModelCatalogRefreshResponse>("model_catalog_refresh", {
    providers: providers?.length ? providers : null,
  });
}

export function capabilityMapFromCatalog(
  models: ModelCapabilityRecord[],
): Map<string, ModelCapabilityRecord> {
  const map = new Map<string, ModelCapabilityRecord>();
  for (const record of models) {
    map.set(modelPickKey(record.provider, record.modelId), record);
  }
  return map;
}

export function formatContextBadge(maxInputTokens?: number | null): string | null {
  if (!maxInputTokens || maxInputTokens <= 0) return null;
  if (maxInputTokens >= 1_000_000) {
    return `${(maxInputTokens / 1_000_000).toFixed(maxInputTokens % 1_000_000 === 0 ? 0 : 1)}M ctx`;
  }
  if (maxInputTokens >= 1_000) {
    return `${Math.round(maxInputTokens / 1_000)}K ctx`;
  }
  return `${maxInputTokens} ctx`;
}

export function formatVisionBadge(supportsVision: boolean): string | null {
  return supportsVision ? "Vision" : null;
}

export function formatPricingBadge(
  pricing?: ModelCapabilityRecord["pricing"],
): string | null {
  const prompt = pricing?.promptPerTokenUsd;
  if (prompt == null || !Number.isFinite(prompt) || prompt <= 0) return null;
  const perMillion = prompt * 1_000_000;
  if (perMillion >= 1) {
    return `$${perMillion.toFixed(perMillion >= 10 ? 0 : 2)}/M in`;
  }
  if (prompt >= 0.001) {
    return `$${(prompt * 1000).toFixed(2)}/K in`;
  }
  return `$${prompt.toExponential(1)} in`;
}

export function badgesForCapability(record: ModelCapabilityRecord): string[] {
  const badges: string[] = [];
  const vision = formatVisionBadge(record.supportsVision);
  if (vision) badges.push(vision);
  const context = formatContextBadge(record.maxInputTokens);
  if (context) badges.push(context);
  const price = formatPricingBadge(record.pricing);
  if (price) badges.push(price);
  return badges;
}

export function badgesForModel(
  map: Map<string, ModelCapabilityRecord>,
  provider: string,
  model: string,
): string[] {
  const record = map.get(modelPickKey(provider, model));
  return record ? badgesForCapability(record) : [];
}
