import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface DisabledBindingRef {
  capabilityId: string;
  source: string;
  reference: string;
}

export interface CapabilitiesOverlayLoadResult {
  path: string;
  fileExists: boolean;
  disabledBindings: DisabledBindingRef[];
  webSearch: {
    preferredProvider?: string | null;
    tryFallbacks?: boolean | null;
  };
}

export interface CapabilitiesMutationResult {
  ok: boolean;
  message: string;
  path: string;
}

export async function loadCapabilitiesOverlay(): Promise<CapabilitiesOverlayLoadResult> {
  if (!isTauri()) {
    return {
      path: "",
      fileExists: false,
      disabledBindings: [],
      webSearch: {},
    };
  }
  return invoke<CapabilitiesOverlayLoadResult>("capabilities_load_overlay");
}

export async function setCapabilityBindingEnabled(request: {
  capabilityId: string;
  source: string;
  reference: string;
  enabled: boolean;
}): Promise<CapabilitiesMutationResult> {
  if (!isTauri()) {
    return { ok: false, message: "Unavailable in browser dev mode", path: "" };
  }
  return invoke<CapabilitiesMutationResult>("capabilities_set_binding_enabled", { request });
}

export async function reindexCapabilities(): Promise<void> {
  if (!isTauri()) return;
  await invoke("catalog_reindex_capabilities");
}

export async function toggleCapabilityBinding(
  capabilityId: string,
  source: string,
  reference: string,
  enabled: boolean,
): Promise<CapabilitiesMutationResult> {
  const result = await setCapabilityBindingEnabled({
    capabilityId,
    source,
    reference,
    enabled,
  });
  await reindexCapabilities();
  return result;
}

export function isBindingDisabled(
  disabled: DisabledBindingRef[],
  capabilityId: string,
  source: string,
  reference: string,
): boolean {
  return disabled.some(
    (entry) =>
      entry.capabilityId.toLowerCase() === capabilityId.toLowerCase() &&
      entry.source.toLowerCase() === source.toLowerCase() &&
      entry.reference === reference,
  );
}
