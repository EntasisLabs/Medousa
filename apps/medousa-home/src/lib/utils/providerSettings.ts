import type { ProviderCatalogEntry } from "$lib/types/providers";
import {
  messagingClearSecret,
  messagingReadSecret,
  messagingSaveSecret,
  messagingSecretStatus,
} from "$lib/messaging";
import {
  CUSTOM_PROVIDER_CATALOG_ID,
  isValidBaseUrl,
  normalizeBaseUrl,
  normalizeCustomProviderId,
} from "$lib/utils/customProvider";

export const CUSTOM_PROVIDER_ID_SECRET = "custom_provider_id";

export function baseUrlSecretId(catalogProviderId: string): string {
  return `base_url_${catalogProviderId.trim().toLowerCase()}`;
}

export function apiKeySecretId(providerId: string): string {
  return `api_key_${providerId.trim().toLowerCase()}`;
}

export function providerAllowsBaseUrl(entry: ProviderCatalogEntry): boolean {
  return (
    entry.supportsCustomBaseUrl ||
    entry.id === "ollama" ||
    entry.id === "medousa-local" ||
    entry.id === CUSTOM_PROVIDER_CATALOG_ID
  );
}

export function providerAllowsApiKey(entry: ProviderCatalogEntry): boolean {
  return entry.needsApiKey || entry.id === CUSTOM_PROVIDER_CATALOG_ID;
}

export function providerIsConfigurable(entry: ProviderCatalogEntry): boolean {
  return providerAllowsBaseUrl(entry) || providerAllowsApiKey(entry);
}

export async function loadCustomProviderId(): Promise<string | null> {
  const raw = await messagingReadSecret(CUSTOM_PROVIDER_ID_SECRET);
  const id = raw ? normalizeCustomProviderId(raw) : "";
  return id || null;
}

export async function saveCustomProviderId(id: string | null): Promise<void> {
  const normalized = id ? normalizeCustomProviderId(id) : "";
  if (normalized) {
    await messagingSaveSecret(CUSTOM_PROVIDER_ID_SECRET, normalized);
  } else {
    await messagingClearSecret(CUSTOM_PROVIDER_ID_SECRET);
  }
}

export async function loadProviderBaseUrlOverride(
  catalogProviderId: string,
): Promise<string | null> {
  const raw = await messagingReadSecret(baseUrlSecretId(catalogProviderId));
  if (!raw?.trim()) return null;
  return normalizeBaseUrl(raw);
}

export async function saveProviderBaseUrlOverride(
  catalogProviderId: string,
  url: string | null,
): Promise<void> {
  const secretId = baseUrlSecretId(catalogProviderId);
  const trimmed = url?.trim() ?? "";
  if (trimmed && isValidBaseUrl(trimmed)) {
    await messagingSaveSecret(secretId, normalizeBaseUrl(trimmed));
  } else {
    await messagingClearSecret(secretId);
  }
}

export async function resolveProviderBaseUrl(
  entry: ProviderCatalogEntry,
): Promise<string | null> {
  const saved = await loadProviderBaseUrlOverride(entry.id);
  if (saved) return saved;
  return entry.defaultBaseUrl?.trim() || null;
}

/** Runtime genai provider id (custom uses configured adapter id). */
export async function resolveRuntimeProviderId(
  catalogProviderId: string,
): Promise<string> {
  if (catalogProviderId === CUSTOM_PROVIDER_CATALOG_ID) {
    return (await loadCustomProviderId()) ?? catalogProviderId;
  }
  return catalogProviderId;
}

export async function isCustomProviderReady(): Promise<boolean> {
  const id = await loadCustomProviderId();
  const baseUrl = await loadProviderBaseUrlOverride(CUSTOM_PROVIDER_CATALOG_ID);
  return Boolean(id && baseUrl && isValidBaseUrl(baseUrl));
}

export async function resolveApiKeySecretId(
  entry: ProviderCatalogEntry,
): Promise<string> {
  if (entry.id === CUSTOM_PROVIDER_CATALOG_ID) {
    const runtimeId = await loadCustomProviderId();
    return apiKeySecretId(runtimeId ?? "custom");
  }
  return apiKeySecretId(entry.id);
}

export interface ProviderSettingsSummary {
  baseUrl: string | null;
  baseUrlIsOverride: boolean;
  hasApiKey: boolean;
  customProviderId: string | null;
  ready: boolean;
}

export async function loadProviderSettingsSummary(
  entry: ProviderCatalogEntry,
): Promise<ProviderSettingsSummary> {
  const savedUrl = await loadProviderBaseUrlOverride(entry.id);
  const defaultUrl = entry.defaultBaseUrl?.trim() || null;
  const baseUrl = savedUrl ?? defaultUrl;
  const keySecret = await resolveApiKeySecretId(entry);
  const hasApiKey = providerAllowsApiKey(entry)
    ? await messagingSecretStatus(keySecret)
    : false;
  const customProviderId =
    entry.id === CUSTOM_PROVIDER_CATALOG_ID ? await loadCustomProviderId() : null;

  let ready = true;
  if (entry.id === CUSTOM_PROVIDER_CATALOG_ID) {
    ready = Boolean(customProviderId && savedUrl && isValidBaseUrl(savedUrl));
  } else if (providerAllowsApiKey(entry) && entry.needsApiKey) {
    ready = hasApiKey;
  }

  return {
    baseUrl,
    baseUrlIsOverride: Boolean(savedUrl),
    hasApiKey,
    customProviderId,
    ready,
  };
}

export function formatProviderSettingsSummary(
  entry: ProviderCatalogEntry,
  summary: ProviderSettingsSummary,
): string {
  const parts: string[] = [];
  if (entry.id === CUSTOM_PROVIDER_CATALOG_ID) {
    if (summary.customProviderId) {
      parts.push(summary.customProviderId);
    } else {
      parts.push("Not configured");
    }
  }
  if (providerAllowsBaseUrl(entry) && summary.baseUrl) {
    const url = summary.baseUrl;
    parts.push(
      summary.baseUrlIsOverride
        ? url.length > 28
          ? `${url.slice(0, 25)}…`
          : url
        : "Default URL",
    );
  }
  if (providerAllowsApiKey(entry)) {
    parts.push(summary.hasApiKey ? "Key stored" : "No key");
  }
  return parts.filter(Boolean).join(" · ") || "Tap to configure";
}
