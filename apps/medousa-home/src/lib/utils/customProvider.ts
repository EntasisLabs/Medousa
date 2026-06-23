import type { InferenceTarget } from "$lib/types/inferenceProfiles";
import type { ProvidersListResult } from "$lib/types/providers";
import { findCatalogProvider } from "$lib/utils/providersApi";

/** Catalog sentinel — actual provider id is user-defined. */
export const CUSTOM_PROVIDER_CATALOG_ID = "custom";

export function normalizeCustomProviderId(value: string): string {
  return value.trim().toLowerCase().replace(/\s+/g, "-");
}

export function isCustomCatalogEntry(providerId: string): boolean {
  return providerId.trim().toLowerCase() === CUSTOM_PROVIDER_CATALOG_ID;
}

export function isCatalogProviderId(
  providerId: string,
  catalog: ProvidersListResult,
): boolean {
  const id = providerId.trim().toLowerCase();
  if (!id || isCustomCatalogEntry(id)) return false;
  return catalog.providers.some((entry) => entry.id.toLowerCase() === id);
}

/** True when the saved profile points at a user-defined endpoint. */
export function isConfiguredCustomProvider(
  selection: InferenceTarget,
  catalog: ProvidersListResult,
): boolean {
  const id = selection.provider.trim().toLowerCase();
  if (!id || isCustomCatalogEntry(id)) return false;
  if (!isCatalogProviderId(id, catalog)) return true;
  const entry = findCatalogProvider(catalog, id);
  const savedUrl = selection.baseUrl?.trim() ?? "";
  if (!savedUrl || !entry?.defaultBaseUrl) return false;
  return savedUrl !== entry.defaultBaseUrl.trim();
}

export function customProviderHint(baseUrl: string | null | undefined): string | null {
  const trimmed = baseUrl?.trim();
  if (!trimmed) return null;
  if (trimmed.length <= 42) return trimmed;
  return `${trimmed.slice(0, 39)}…`;
}

export function normalizeBaseUrl(value: string): string {
  return value.trim().replace(/\/+$/, "");
}

export function isValidBaseUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  try {
    const url = new URL(trimmed);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}
