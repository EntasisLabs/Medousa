import { invoke } from "@tauri-apps/api/core";
import { isBrowserWorkshop } from "$lib/platform";
import { isTauri } from "$lib/window";
import {
  PERSONAL_WORKSHOP_ID,
  defaultWorkshopRegistry,
  findWorkshop,
  parseWorkshopRegistry,
  type WorkshopIcon,
  type WorkshopRegistry,
  type WorkshopServer,
} from "$lib/types/workshopRegistry";
import {
  activateBrowserPortal,
  forgetBrowserPortal,
  loadBrowserWorkshopRegistry,
  saveBrowserWorkshopRegistry,
} from "$lib/wasm/browserPortal";

function browserRegistry(): WorkshopRegistry {
  return loadBrowserWorkshopRegistry() ?? defaultWorkshopRegistry();
}

function persistBrowserRegistry(registry: WorkshopRegistry): WorkshopRegistry {
  saveBrowserWorkshopRegistry(registry);
  return registry;
}

export function upsertBrowserPortalWorkshop(
  registry: WorkshopRegistry,
  workshop: WorkshopServer,
): WorkshopRegistry {
  const existing = registry.workshops.findIndex((item) => item.id === workshop.id);
  if (existing >= 0) {
    const previous = registry.workshops[existing];
    registry.workshops[existing] = {
      ...previous,
      ...workshop,
      createdAt: previous.createdAt,
    };
  } else {
    registry.workshops.push(workshop);
  }
  return persistBrowserRegistry(registry);
}

export async function loadWorkshopRegistry(): Promise<WorkshopRegistry> {
  if (isBrowserWorkshop()) return browserRegistry();
  if (!isTauri()) return defaultWorkshopRegistry();
  const raw = await invoke<unknown>("workshops_load");
  return parseWorkshopRegistry(raw) ?? defaultWorkshopRegistry();
}

export async function setActiveWorkshop(workshopId: string): Promise<WorkshopRegistry> {
  if (isBrowserWorkshop()) {
    const registry = browserRegistry();
    if (!findWorkshop(registry, workshopId)) {
      throw new Error("Unknown workshop");
    }
    registry.activeWorkshopId = workshopId;
    const saved = persistBrowserRegistry(registry);
    await activateBrowserPortal(workshopId);
    return saved;
  }
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

export async function addLocalWorkshop(
  label: string,
  dataDir: string,
): Promise<WorkshopRegistry> {
  if (!isTauri()) return defaultWorkshopRegistry();
  const raw = await invoke<unknown>("workshops_add_local", { label, dataDir });
  const parsed = parseWorkshopRegistry(raw);
  if (!parsed) throw new Error("Invalid workshop registry response");
  return parsed;
}

export async function renameWorkshop(
  workshopId: string,
  label: string,
): Promise<WorkshopRegistry> {
  if (isBrowserWorkshop()) {
    const registry = browserRegistry();
    const workshop = findWorkshop(registry, workshopId);
    if (!workshop) throw new Error("Unknown workshop");
    workshop.label = label.trim() || workshop.label;
    workshop.updatedAt = new Date().toISOString();
    return persistBrowserRegistry(registry);
  }
  if (!isTauri()) return defaultWorkshopRegistry();
  const raw = await invoke<unknown>("workshops_rename", { workshopId, label });
  const parsed = parseWorkshopRegistry(raw);
  if (!parsed) throw new Error("Invalid workshop registry response");
  return parsed;
}

export async function removeWorkshop(workshopId: string): Promise<WorkshopRegistry> {
  if (isBrowserWorkshop()) {
    const registry = browserRegistry();
    registry.workshops = registry.workshops.filter((workshop) => workshop.id !== workshopId);
    if (registry.activeWorkshopId === workshopId) {
      registry.activeWorkshopId = PERSONAL_WORKSHOP_ID;
    }
    const saved = persistBrowserRegistry(registry);
    await forgetBrowserPortal(workshopId);
    if (saved.activeWorkshopId === PERSONAL_WORKSHOP_ID) {
      await activateBrowserPortal(PERSONAL_WORKSHOP_ID);
    }
    return saved;
  }
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
  if (isBrowserWorkshop()) {
    const registry = browserRegistry();
    const workshop = findWorkshop(registry, workshopId);
    if (!workshop) throw new Error("Unknown workshop");
    workshop.clientState = {
      ...workshop.clientState,
      ...(patch.lastSessionId !== undefined ? { lastSessionId: patch.lastSessionId } : {}),
      ...(patch.colorThemeId !== undefined
        ? { colorThemeId: patch.colorThemeId ?? undefined }
        : {}),
    };
    workshop.updatedAt = new Date().toISOString();
    return persistBrowserRegistry(registry);
  }
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
  if (isBrowserWorkshop()) {
    const registry = browserRegistry();
    const workshop = findWorkshop(registry, workshopId);
    if (!workshop) throw new Error("Unknown workshop");
    if (patch.icon !== undefined) workshop.icon = patch.icon ?? undefined;
    if (patch.brandColor !== undefined) workshop.brandColor = patch.brandColor ?? undefined;
    if (patch.tagline !== undefined) workshop.tagline = patch.tagline ?? undefined;
    workshop.updatedAt = new Date().toISOString();
    return persistBrowserRegistry(registry);
  }
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
