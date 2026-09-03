/**
 * Shared provider → model list resolution (settings ModelCatalogSheet + onboarding ProviderPicker).
 * Order: connected-account catalog → daemon capability catalog → live provider listing →
 * catalog defaultModel.
 */

import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
import type { ProviderCatalogEntry } from "$lib/types/providers";
import { CUSTOM_PROVIDER_CATALOG_ID } from "$lib/utils/customProvider";
import {
  defaultProviderRecords,
  listModelCatalog,
  recordsFromModelIds,
} from "$lib/utils/modelCapabilityCatalog";
import { listChatGptOAuthModels } from "$lib/utils/chatgptOAuth";
import { listProviderModels } from "$lib/utils/providersApi";
import {
  resolveProviderBaseUrl,
  resolveRuntimeProviderId,
} from "$lib/utils/providerSettings";

export type ResolveProviderModelsOptions = {
  /** Filter daemon catalog by capability (e.g. "vision"). */
  capability?: "vision" | "text";
  /** Inline API key (onboarding) — passed to live listing when set. */
  apiKey?: string;
  /** Override base URL (onboarding / unsaved draft). */
  baseUrl?: string;
};

export async function resolveModelsForProvider(
  entry: ProviderCatalogEntry,
  options?: ResolveProviderModelsOptions,
): Promise<ModelCapabilityRecord[]> {
  const runtimeId = await resolveRuntimeProviderId(entry.id).catch(() => entry.id);
  const savedBaseUrl = await resolveProviderBaseUrl(entry).catch(() => null);
  const baseUrl =
    options?.baseUrl?.trim() ||
    savedBaseUrl ||
    entry.defaultBaseUrl?.trim() ||
    undefined;

  if (entry.id.trim().toLowerCase() === "openai-codex") {
    try {
      const accountCatalog = await listChatGptOAuthModels();
      const records = recordsFromModelIds(
        runtimeId,
        accountCatalog.models,
        "chatgpt-account",
      );
      const compatible = filterRecordsForCapability(records, options?.capability);
      if (compatible.length > 0) return compatible;
    } catch {
      // The account may be signed out or temporarily offline. Continue through
      // the daemon snapshot before falling back to the provider default.
    }
  }

  if (entry.id !== CUSTOM_PROVIDER_CATALOG_ID) {
    try {
      const capabilityRaw = options?.capability?.trim();
      const capability =
        capabilityRaw === "vision" || capabilityRaw === "text"
          ? capabilityRaw
          : undefined;
      const response = await listModelCatalog({
        provider: entry.id,
        capability,
      });
      const fromCatalog = response.models.filter(
        (record) =>
          record.provider.trim().toLowerCase() === entry.id.toLowerCase() ||
          record.provider.trim().toLowerCase() === runtimeId.toLowerCase(),
      );
      if (fromCatalog.length > 0) {
        return filterRecordsForCapability(fromCatalog, options?.capability);
      }
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
      return filterRecordsForCapability(
        recordsFromModelIds(runtimeId, live.models, live.source),
        options?.capability,
      );
    }
  } catch {
    // Fall through to default.
  }

  return filterRecordsForCapability(
    defaultProviderRecords({
      ...entry,
      id: runtimeId,
      defaultModel: entry.defaultModel,
    }),
    options?.capability,
  );
}

export function filterRecordsForCapability(
  records: ModelCapabilityRecord[],
  capability?: ResolveProviderModelsOptions["capability"],
): ModelCapabilityRecord[] {
  if (capability === "vision") {
    return records.filter((record) => record.supportsVision);
  }
  return records;
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
