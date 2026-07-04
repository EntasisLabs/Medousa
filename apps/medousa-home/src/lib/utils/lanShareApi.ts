import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface DiscoveredWorkshop {
  instanceName: string;
  host: string;
  port: number;
  deviceId?: string | null;
  peerName?: string | null;
  protocolVersion?: string | null;
  capabilityFlags?: string | null;
  authRequired?: boolean | null;
  modelDescriptor?: string | null;
  daemonUrl: string;
}

export interface LanWorkshopsResponse {
  workshops: DiscoveredWorkshop[];
  browseMs: number;
}

export interface TrustedWorkshopSummary {
  workshopId: string;
  label: string;
  daemonUrl: string;
  workshopDeviceId: string;
  pairedAt: string;
  hasSessionToken: boolean;
  hasIrohTicket: boolean;
}

export type ShareConflictStrategy = "skip" | "rename" | "overwrite";

export interface ShareExportRequest {
  artifactIds?: string[];
  vaultPaths?: string[];
  includeEnvironment?: boolean;
  surfaceIds?: string[];
  componentIds?: string[];
  profileId?: string | null;
}

export interface ShareImportRequest {
  bundle: Record<string, unknown>;
  conflictStrategy?: ShareConflictStrategy;
  profileId?: string | null;
}

export interface ShareImportResult {
  artifactsImported: number;
  artifactsSkipped: number;
  vaultNotesImported: number;
  vaultNotesSkipped: number;
  surfacesImported: number;
  componentsImported: number;
  layoutPresetsImported: number;
  artifactIdMap?: Array<[string, string]>;
  warnings?: string[];
}

export interface TrustWorkshopResult {
  pairingId: string;
  phoneId: string;
  workshopDeviceId: string;
  workshopId: string;
  workshopPeerName: string;
  daemonUrl: string;
}

function requireTauri(): void {
  if (!isTauri()) {
    throw new Error("LAN sharing requires the Medousa desktop app");
  }
}

export async function discoverLanWorkshops(): Promise<LanWorkshopsResponse> {
  requireTauri();
  return invoke<LanWorkshopsResponse>("lan_discover_workshops");
}

export async function listTrustedWorkshops(): Promise<TrustedWorkshopSummary[]> {
  requireTauri();
  return invoke<TrustedWorkshopSummary[]>("list_trusted_workshops");
}

export async function trustWorkshopFromQr(input: {
  qrUrl: string;
  daemonUrl: string;
  workshopName?: string | null;
}): Promise<TrustWorkshopResult> {
  requireTauri();
  return invoke<TrustWorkshopResult>("trust_workshop_from_qr", {
    request: {
      qrUrl: input.qrUrl,
      daemonUrl: input.daemonUrl,
      workshopName: input.workshopName ?? null,
    },
  });
}

export async function revokeTrustedWorkshop(workshopId: string): Promise<void> {
  requireTauri();
  await invoke("revoke_trusted_workshop", { workshopId });
}

export async function exportShareBundle(
  request: ShareExportRequest,
): Promise<Record<string, unknown>> {
  requireTauri();
  return invoke<Record<string, unknown>>("share_export_bundle", { request });
}

export async function importShareBundle(
  request: ShareImportRequest,
): Promise<ShareImportResult> {
  requireTauri();
  return invoke<ShareImportResult>("share_import_bundle", { request });
}

export async function pushShareBundleToWorkshop(input: {
  workshopId: string;
  bundle: Record<string, unknown>;
  conflictStrategy?: ShareConflictStrategy;
  profileId?: string | null;
}): Promise<ShareImportResult> {
  requireTauri();
  return invoke<ShareImportResult>("share_push_to_workshop", {
    request: {
      workshopId: input.workshopId,
      bundle: input.bundle,
      conflictStrategy: input.conflictStrategy ?? "rename",
      profileId: input.profileId ?? null,
    },
  });
}

export function capabilityBadges(flags?: string | null): string[] {
  if (!flags) return [];
  const value = Number.parseInt(flags, 16);
  if (!Number.isFinite(value)) return [];
  const badges: string[] = [];
  if (value & 0x0008) badges.push("Share");
  if (value & 0x0010) badges.push("Layouts");
  if (value & 0x0020) badges.push("Relay");
  return badges;
}

export function downloadShareBundle(bundle: Record<string, unknown>, filename?: string): void {
  const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename ?? `medousa-share-${new Date().toISOString().slice(0, 10)}.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}
