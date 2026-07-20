/**
 * Shared provider → model list resolution (settings ModelCatalogSheet + onboarding ProviderPicker).
 * Order: daemon capability catalog → live provider listing → catalog defaultModel.
 */

import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
import type { ProviderCatalogEntry } from "$lib/types/providers";
import { CUSTOM_PROVIDER_CATALOG_ID } from "$lib/utils/customProvider";
import {
  defaultProviderRecords,
  listModelCatalog,
  recordsFromModelIds,
} from "$lib/utils/modelCapabilityCatalog";
import { listProviderModels } from "$lib/utils/providersApi";
import {
  resolveProviderBaseUrl,
  resolveRuntimeProviderId,
} from "$lib/utils/providerSettings";

export type ResolveProviderModelsOptions = {
  /** Filter daemon catalog by capability (e.g. "vision"). */
  capability?: string;
  /** Inline API key (onboarding) — passed to live listing when set. */
  apiKey?: string;
  /** Override base URL (onboarding / unsaved draft). */
  baseUrl?: string;
};

export async function resolveModelsForProvider(
  entry: ProviderCatalogEntry,
  options?: ResolveProviderModelsOptions,
): Promise<ModelCapabilityRecord[]> {
  const runtimeId = await resolveRuntimeProviderId(entry.id);
  const baseUrl =
    options?.baseUrl?.trim() ||
    (await resolveProviderBaseUrl(entry)) ||
    entry.defaultBaseUrl?.trim() ||
    undefined;

  if (entry.id !== CUSTOM_PROVIDER_CATALOG_ID) {
    try {
      const response = await listModelCatalog({
        provider: entry.id,
        capability: options?.capability?.trim() || undefined,
      });
      const fromCatalog = response.models.filter(
        (record) =>
          record.provider.trim().toLowerCase() === entry.id.toLowerCase() ||
          record.provider.trim().toLowerCase() === runtimeId.toLowerCase(),
      );
      if (fromCatalog.length > 0) return fromCatalog;
    } catch {
      // Fall through to live listing.
    }
  }

  try {
    const live = await listProviderModels({
      provider: runtimeId,
      apiKey: options?.apiKey?.trim() || undefined,
      baseUrl: baseUrl || undefined,
    });
    if (live.models.length > 0) {
      return recordsFromModelIds(runtimeId, live.models, live.source);
    }
  } catch {
    // Fall through to default.
  }

  return defaultProviderRecords({
    ...entry,
    id: runtimeId,
    defaultModel: entry.defaultModel,
  });
}

/** Pick a model id from resolved records (prefer suggested, then current if still valid, then first). */
export function pickModelFromRecords(
  records: ModelCapabilityRecord[],
  options?: {
    preferred?: string | null;
    current?: string | null;
    fallbackDefault?: string | null;
  },
): string {
  const preferred = options?.preferred?.trim();
  if (preferred && records.some((r) => r.modelId === preferred)) {
    return preferred;
  }
  const current = options?.current?.trim();
  if (current && records.some((r) => r.modelId === current)) {
    return current;
  }
  if (records[0]?.modelId) return records[0].modelId;
  return (
    options?.fallbackDefault?.trim() ||
    preferred ||
    current ||
    ""
  );
}
