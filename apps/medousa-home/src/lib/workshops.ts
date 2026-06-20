import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";
import {
  defaultWorkshopRegistry,
  parseWorkshopRegistry,
  type WorkshopIcon,
  type WorkshopRegistry,
} from "$lib/types/workshopRegistry";

export async function loadWorkshopRegistry(): Promise<WorkshopRegistry> {
  if (!isTauri()) return defaultWorkshopRegistry();
  const raw = await invoke<unknown>("workshops_load");
  return parseWorkshopRegistry(raw) ?? defaultWorkshopRegistry();
}

export async function setActiveWorkshop(workshopId: string): Promise<WorkshopRegistry> {
  if (!isTauri()) {
    const registry = defaultWorkshopRegistry();
    registry.activeWorkshopId = workshopId;
    return registry;
  }
  const raw = await invoke<unknown>("workshops_set_active", { workshopId });
  const parsed = parseWorkshopRegistry(raw);
  if (!parsed) throw new Error("Invalid workshop registry response");
  return parsed;
}

export async function renameWorkshop(
  workshopId: string,
  label: string,
): Promise<WorkshopRegistry> {
  if (!isTauri()) return defaultWorkshopRegistry();
  const raw = await invoke<unknown>("workshops_rename", { workshopId, label });
  const parsed = parseWorkshopRegistry(raw);
  if (!parsed) throw new Error("Invalid workshop registry response");
  return parsed;
}

export async function removeWorkshop(workshopId: string): Promise<WorkshopRegistry> {
  if (!isTauri()) return defaultWorkshopRegistry();
  const raw = await invoke<unknown>("workshops_remove", { workshopId });
  const parsed = parseWorkshopRegistry(raw);
  if (!parsed) throw new Error("Invalid workshop registry response");
  return parsed;
}

export async function updateWorkshopClientState(
  workshopId: string,
  patch: { lastSessionId?: string; colorThemeId?: string | null },
): Promise<WorkshopRegistry> {
  if (!isTauri()) return defaultWorkshopRegistry();
  const args: Record<string, unknown> = { workshopId };
  if (patch.lastSessionId !== undefined) {
    args.lastSessionId = patch.lastSessionId;
  }
  if (patch.colorThemeId !== undefined) {
    args.colorThemeId = patch.colorThemeId;
  }
  const raw = await invoke<unknown>("workshops_update_client_state", args);
  const parsed = parseWorkshopRegistry(raw);
  if (!parsed) throw new Error("Invalid workshop registry response");
  return parsed;
}

export async function updateWorkshopBranding(
  workshopId: string,
  patch: {
    icon?: WorkshopIcon | null;
    brandColor?: string | null;
    tagline?: string | null;
  },
): Promise<WorkshopRegistry> {
  if (!isTauri()) return defaultWorkshopRegistry();
  const args: Record<string, unknown> = { workshopId };
  if (patch.icon !== undefined) args.icon = patch.icon;
  if (patch.brandColor !== undefined) args.brandColor = patch.brandColor;
  if (patch.tagline !== undefined) args.tagline = patch.tagline;
  const raw = await invoke<unknown>("workshops_update_branding", args);
  const parsed = parseWorkshopRegistry(raw);
  if (!parsed) throw new Error("Invalid workshop registry response");
  return parsed;
}
