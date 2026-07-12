/** Whether Home and the active workshop daemon share this machine’s disk. */

import { workshops } from "$lib/stores/workshops.svelte";
import { isTauri } from "$lib/platform";

/**
 * Local workshops run on this device — Home folder pickers, Reveal, and
 * convertFileSrc are safe. Portal/paired workshops point at another host’s disk.
 */
export function isCoLocatedWorkshop(): boolean {
  if (!isTauri()) {
    // Browser shell never has the daemon’s filesystem.
    return false;
  }
  const kind = workshops.activeWorkshop?.kind;
  return kind === "local" || kind == null;
}

export function vaultHostSideHint(): string {
  return "Available on the workshop Mac — open Medousa there to pin folders or preview local files.";
}

export function vaultAddRootRemoteHint(): string {
  return "Add vault folders on the workshop Mac. This connection can’t see folders on this device.";
}

export function vaultPinFolderRemoteHint(): string {
  return "Pin folders on the workshop Mac. Your files here stay on this device.";
}
