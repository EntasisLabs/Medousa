import type { ProviderCatalogEntry } from "$lib/types/providers";
import {
  CUSTOM_PROVIDER_CATALOG_ID,
  isValidBaseUrl,
  normalizeBaseUrl,
  normalizeCustomProviderId,
} from "$lib/utils/customProvider";
import {
  deleteIntegrationSecret,
  ensureConnection,
  findConnectionByKind,
  findCustomConnection,
  putIntegrationSecret,
  type IntegrationConnection,
} from "$lib/utils/integrations";

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

async function connectionForCatalog(
  catalogProviderId: string,
): Promise<IntegrationConnection | null> {
  if (catalogProviderId === CUSTOM_PROVIDER_CATALOG_ID) {
    return findCustomConnection();
  }
  return findConnectionByKind(catalogProviderId);
}

export async function loadCustomProviderId(): Promise<string | null> {
  const connection = await findCustomConnection();
  const id = connection?.kind ? normalizeCustomProviderId(connection.kind) : "";
  if (id && id !== CUSTOM_PROVIDER_CATALOG_ID) return id;
  const catalogId = connection?.catalog_id
    ? normalizeCustomProviderId(connection.catalog_id)
    : "";
  return catalogId && catalogId !== CUSTOM_PROVIDER_CATALOG_ID ? catalogId : null;
}

export async function saveCustomProviderId(id: string | null): Promise<void> {
  const normalized = id ? normalizeCustomProviderId(id) : "";
  if (!normalized) return;
  await ensureConnection(normalized, { catalogId: CUSTOM_PROVIDER_CATALOG_ID });
}

export async function loadProviderBaseUrlOverride(
  catalogProviderId: string,
): Promise<string | null> {
  const connection = await connectionForCatalog(catalogProviderId);
  const raw = connection?.base_url?.trim() ?? "";
  if (!raw) return null;
  return normalizeBaseUrl(raw);
}

export async function saveProviderBaseUrlOverride(
  catalogProviderId: string,
  url: string | null,
): Promise<void> {
  const trimmed = url?.trim() ?? "";
  const baseUrl = trimmed && isValidBaseUrl(trimmed) ? normalizeBaseUrl(trimmed) : null;
  if (catalogProviderId === CUSTOM_PROVIDER_CATALOG_ID) {
    const runtimeId = (await loadCustomProviderId()) ?? CUSTOM_PROVIDER_CATALOG_ID;
    await ensureConnection(runtimeId, {
      catalogId: CUSTOM_PROVIDER_CATALOG_ID,
      baseUrl,
    });
    return;
  }
  await ensureConnection(catalogProviderId, { baseUrl });
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

export async function saveProviderApiKey(
  catalogProviderId: string,
  value: string | null,
): Promise<void> {
  const runtimeId =
    catalogProviderId === CUSTOM_PROVIDER_CATALOG_ID
      ? (await loadCustomProviderId()) ?? CUSTOM_PROVIDER_CATALOG_ID
      : catalogProviderId;
  const extras =
    catalogProviderId === CUSTOM_PROVIDER_CATALOG_ID
      ? { catalogId: CUSTOM_PROVIDER_CATALOG_ID }
      : {};
  const connection = await ensureConnection(runtimeId, extras);
  const trimmed = value?.trim() ?? "";
  if (trimmed) {
    await putIntegrationSecret(connection.connection_id, "api_key", trimmed);
  } else {
    await deleteIntegrationSecret(connection.connection_id, "api_key");
  }
}

export async function providerApiKeyConfigured(
  catalogProviderId: string,
): Promise<boolean> {
  const connection = await connectionForCatalog(catalogProviderId);
  if (connection?.secrets.api_key) return true;
  if (catalogProviderId === CUSTOM_PROVIDER_CATALOG_ID) {
    const runtimeId = await loadCustomProviderId();
    if (runtimeId) {
      return Boolean((await findConnectionByKind(runtimeId))?.secrets.api_key);
    }
  }
  return false;
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
  const hasApiKey = providerAllowsApiKey(entry)
    ? await providerApiKeyConfigured(entry.id)
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
