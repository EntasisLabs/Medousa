import type { TuiDefaults } from "$lib/types/workshopDefaults";
import type { ProvidersListResult } from "$lib/types/providers";
import { findCatalogProvider } from "$lib/utils/providersApi";
import { resolveModelDisplayLabel } from "$lib/utils/modelCatalog";
import { profileForKind, type ProfileKind } from "$lib/utils/modelAssignment";

export type StatusLevel = "ok" | "warn" | "neutral";

export interface RoleStatusChip {
  id: ProfileKind;
  label: string;
  level: StatusLevel;
  detail: string;
}

function providerKeyReady(
  provider: string,
  catalog: ProvidersListResult | null,
  keyStatus: Record<string, boolean>,
): boolean {
  const id = provider.trim().toLowerCase();
  if (!id || id === "ollama" || id === "local") return true;
  const entry = catalog ? findCatalogProvider(catalog, id) : undefined;
  if (entry && !entry.needsApiKey) return true;
  return keyStatus[id] === true;
}

export function modelsWorkshopStatus(
  draft: TuiDefaults,
  catalog: ProvidersListResult | null,
  keyStatus: Record<string, boolean>,
  sttReady: boolean,
): RoleStatusChip[] {
  const main = profileForKind(draft, "main");
  const vision = profileForKind(draft, "vision");
  const stt = profileForKind(draft, "stt");

  const chatModel = main?.model?.trim();
  const chatProvider = main?.provider?.trim() ?? "";
  const chatKeyOk = providerKeyReady(chatProvider, catalog, keyStatus);
  const chatOk = Boolean(chatModel) && chatKeyOk;

  const visionSet = Boolean(vision?.provider?.trim() && vision?.model?.trim());
  const visionKeyOk = visionSet
    ? providerKeyReady(vision!.provider, catalog, keyStatus)
    : false;
  const visionOk = visionSet && visionKeyOk;

  const sttSet = Boolean(stt?.provider?.trim() && stt?.model?.trim());
  const sttOk = sttReady && sttSet;

  return [
    {
      id: "main",
      label: "Chat",
      level: chatOk ? "ok" : chatModel && !chatKeyOk ? "warn" : chatModel ? "ok" : "warn",
      detail: chatOk
        ? resolveModelDisplayLabel(chatProvider, chatModel!)
        : chatModel && !chatKeyOk
          ? "Needs key"
          : "Not set",
    },
    {
      id: "vision",
      label: "Vision",
      level: visionOk ? "ok" : visionSet && !visionKeyOk ? "warn" : "neutral",
      detail: visionOk
        ? resolveModelDisplayLabel(vision!.provider, vision!.model)
        : visionSet && !visionKeyOk
          ? "Needs key"
          : "Optional",
    },
    {
      id: "stt",
      label: "Dictation",
      level: sttOk ? "ok" : sttSet ? "warn" : "neutral",
      detail: sttOk
        ? resolveModelDisplayLabel(stt!.provider, stt!.model)
        : sttSet
          ? "Needs key"
          : "Optional",
    },
  ];
}

export function fallbackSummaryLabel(
  draft: TuiDefaults,
  profile: ProfileKind,
  catalog: ProvidersListResult | null,
): string {
  const configured =
    profileForKind(draft, profile)?.fallbacks?.filter(
      (entry) => entry.provider.trim() && entry.model.trim(),
    ) ?? [];
  if (configured.length === 0) return "Optional";
  if (configured.length === 1) {
    return resolveModelDisplayLabel(configured[0]!.provider, configured[0]!.model);
  }
  return `${configured.length} backups`;
}
