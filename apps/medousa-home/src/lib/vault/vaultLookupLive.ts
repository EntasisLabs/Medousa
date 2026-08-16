/** Live H07 vault lookup snapshot — published by the vault store, read by features. */

import {
  buildVaultLookupSnapshot,
  type VaultLookupSnapshot,
} from "$lib/utils/vaultLookup";

let snapshot: VaultLookupSnapshot = buildVaultLookupSnapshot([], 0);

export function publishVaultLookupSnapshot(next: VaultLookupSnapshot): void {
  snapshot = next;
}

export function getVaultLookupSnapshot(): VaultLookupSnapshot {
  return snapshot;
}
