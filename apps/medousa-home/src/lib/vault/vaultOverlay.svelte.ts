/** Light flags so AppShell can lazy-mount vault chrome without the vault store. */
class VaultOverlayStore {
  garageWizardOpen = $state(false);
  attachmentPanelOpen = $state(false);
}

export const vaultOverlay = new VaultOverlayStore();
