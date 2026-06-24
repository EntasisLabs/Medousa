class VaultQuickSwitcherStore {
  open = $state(false);

  openSwitcher() {
    this.open = true;
  }

  closeSwitcher() {
    this.open = false;
  }

  toggle() {
    this.open = !this.open;
  }
}

export const vaultQuickSwitcher = new VaultQuickSwitcherStore();
