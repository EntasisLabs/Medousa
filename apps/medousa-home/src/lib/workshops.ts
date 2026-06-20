import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";
import {
  defaultWorkshopRegistry,
  parseWorkshopRegistry,
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
  clientState: { lastSessionId?: string | null },
): Promise<WorkshopRegistry> {
  if (!isTauri()) return defaultWorkshopRegistry();
  const raw = await invoke<unknown>("workshops_update_client_state", {
    workshopId,
    lastSessionId: clientState.lastSessionId ?? null,
  });
  const parsed = parseWorkshopRegistry(raw);
  if (!parsed) throw new Error("Invalid workshop registry response");
  return parsed;
}
