export type VaultContextTarget =
  | { kind: "note"; path: string }
  | { kind: "attachment"; path: string; notePath: string };

export class VaultContextMenuStore {
  open = $state(false);
  x = $state(0);
  y = $state(0);
  target = $state<VaultContextTarget | null>(null);

  showAt(clientX: number, clientY: number, target: VaultContextTarget) {
    this.x = clientX;
    this.y = clientY;
    this.target = target;
    this.open = true;
  }

  showNote(path: string, clientX: number, clientY: number) {
    this.showAt(clientX, clientY, { kind: "note", path });
  }

  showAttachment(path: string, notePath: string, clientX: number, clientY: number) {
    this.showAt(clientX, clientY, { kind: "attachment", path, notePath });
  }

  close() {
    this.open = false;
    this.target = null;
  }
}

export const vaultContextMenu = new VaultContextMenuStore();
