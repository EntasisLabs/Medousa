/** Whether Home and the active workshop daemon share this machine’s disk. */

import { isTauri } from "$lib/platform";

let kindPort: () => string | undefined = () => undefined;
let idPort: () => string | undefined = () => undefined;

/** Bound by shell lifecycle so this helper does not import the workshops store. */
export function setActiveWorkshopKindPort(port: () => string | undefined): void {
  kindPort = port;
}

export function setActiveWorkshopIdPort(port: (() => string | undefined) | null): void {
  idPort = port ?? (() => undefined);
}

export function activeWorkshopId(): string {
  return idPort()?.trim() || "personal";
}

/** Local client caches must never alias the same daemon id across workshops. */
export function workshopScopedStorageKey(prefix: string, workshopId?: string): string {
  const scope = workshopId?.trim() || activeWorkshopId();
  return `${prefix}:${scope}`;
}

/**
 * Local workshops run on this device — Home folder pickers, Reveal, and
 * trusted local-resource previews are available. Portal/paired workshops point at another host’s disk.
 */
export function isCoLocatedWorkshop(): boolean {
  if (!isTauri()) {
    // Browser shell never has the daemon’s filesystem.
    return false;
  }
  // Unknown kind is treated as remote — assuming local hands Home paths to a
  // daemon that may not share this disk.
  return kindPort() === "local";
}

export function vaultHostSideHint(): string {
  return "Available on the workshop machine — open Medousa there to pin folders or preview local files.";
}

export function vaultAddRootRemoteHint(): string {
  return "Add vault folders on the workshop machine. This connection can’t see folders on this device.";
}

export function vaultPinFolderRemoteHint(): string {
  return "Pin folders on the workshop machine. Your files here stay on this device.";
}
