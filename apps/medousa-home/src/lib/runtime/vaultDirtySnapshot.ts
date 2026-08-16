let dirty = false;

export function isVaultDirty(): boolean {
  return dirty;
}

export function publishVaultDirty(next: boolean): void {
  dirty = next;
}
