import type { VaultStore } from "$lib/stores/vault.svelte";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";
import { loadVaultRecent, rememberVaultRecent } from "$lib/utils/vaultRecent";

export function vaultNoteStorageKey(rootId: string | null): string {
  return workshopScopedStorageKey(`medousa-home-last-note:${encodeURIComponent(rootId ?? "personal")}`);
}
export function loadLastNote(rootId: string | null): string | null {
  if (typeof localStorage === "undefined" || !rootId) return null;
  return localStorage.getItem(vaultNoteStorageKey(rootId));
}
export function restoreVaultRootPreferences(host: VaultStore) {
  host.recentPaths = loadVaultRecent(vaultNoteStorageKey(host.activeVaultRootId));
}
export function rememberOpenedVaultNote(host: VaultStore, path: string) {
  const key = vaultNoteStorageKey(host.activeVaultRootId);
  localStorage.setItem(key, path);
  rememberVaultRecent(path, key);
  host.recentPaths = loadVaultRecent(key);
}
export async function resetVaultRootContext(host: VaultStore, resetBrowse: () => void, resetRoots: () => void) {
  host.workshopEpoch += 1;
  host.openGeneration += 1;
  host.vaultGeneration = 0;
  host.listingIncomplete = false;
  resetBrowse(); resetRoots();
  host.vaultTags = []; host.recentPaths = []; host.activeSpaceFilter = null;
  host.resetSaveState(); host.rebuildLookupSnapshot(); host.bumpContentSync(); host.bumpNoteBuffers();
  const epoch = host.workshopEpoch;
  const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
  if (epoch === host.workshopEpoch) lmeWorkspace.detachVaultNotes();
}
