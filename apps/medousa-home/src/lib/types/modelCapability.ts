/** Daemon model capability registry — mirrors `Medousa/src/model_capability_registry/types.rs`. */

export type Modality = "text" | "image" | "audio" | "file" | "video";

export interface ModelPricing {
  promptPerTokenUsd?: number | null;
  completionPerTokenUsd?: number | null;
  imagePerUnitUsd?: number | null;
}

export interface ModelCapabilityRecord {
  provider: string;
  modelId: string;
  displayName?: string | null;
  inputModalities: Modality[];
  outputModalities: Modality[];
  maxInputTokens?: number | null;
  maxOutputTokens?: number | null;
  supportsToolCalling?: boolean | null;
  supportsVision: boolean;
  pricing?: ModelPricing | null;
  source: string;
  fetchedAt: string;
}

export interface CatalogProviderFreshness {
  provider: string;
  fetchedAt?: string | null;
  modelCount: number;
  source: string;
  stale: boolean;
  error?: string | null;
}

export interface CatalogFreshnessResponse {
  ttlSecs: number;
  providers: CatalogProviderFreshness[];
}

export interface ModelCatalogListResponse {
  freshness: CatalogFreshnessResponse;
  models: ModelCapabilityRecord[];
}

export interface ModelCapabilitiesLookupResponse {
  found: boolean;
  model?: ModelCapabilityRecord | null;
  heuristic: boolean;
}

export interface ModelCatalogRefreshFailure {
  provider: string;
  message: string;
}

export interface ModelCatalogRefreshResponse {
  refreshed: string[];
  failures: ModelCatalogRefreshFailure[];
  freshness: CatalogFreshnessResponse;
}
