/** Whether Home and the active workshop daemon share this machine’s disk. */

import { workshops } from "$lib/stores/workshops.svelte";
import { isTauri } from "$lib/platform";

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
  return workshops.activeWorkshop?.kind === "local";
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
