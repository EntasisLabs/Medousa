import type { AllowedSurfaceIcon } from "$lib/utils/environmentIconCatalog";

export type VaultContextTarget =
  | { kind: "note"; path: string }
  | { kind: "attachment"; path: string; notePath: string }
  | {
      kind: "folder";
      iconKey: string;
      label: string;
      spaceId?: string | null;
    };

export class VaultContextMenuStore {
  open = $state(false);
  x = $state(0);
  y = $state(0);
  target = $state<VaultContextTarget | null>(null);
  /** When set, the menu shows the icon picker for this folder key. */
  iconPickerKey = $state<string | null>(null);
  iconPickerLabel = $state<string>("");

  showAt(clientX: number, clientY: number, target: VaultContextTarget) {
    this.x = clientX;
    this.y = clientY;
    this.target = target;
    this.iconPickerKey = null;
    this.iconPickerLabel = "";
    this.open = true;
  }

  showNote(path: string, clientX: number, clientY: number) {
    this.showAt(clientX, clientY, { kind: "note", path });
  }

  showAttachment(path: string, notePath: string, clientX: number, clientY: number) {
    this.showAt(clientX, clientY, { kind: "attachment", path, notePath });
  }

  showFolder(
    iconKey: string,
    label: string,
    clientX: number,
    clientY: number,
    spaceId?: string | null,
  ) {
    this.showAt(clientX, clientY, {
      kind: "folder",
      iconKey,
      label,
      spaceId: spaceId ?? null,
    });
  }

  openIconPicker(iconKey: string, label: string) {
    this.iconPickerKey = iconKey;
    this.iconPickerLabel = label;
    this.open = true;
  }

  close() {
    this.open = false;
    this.target = null;
    this.iconPickerKey = null;
    this.iconPickerLabel = "";
  }
}

export const vaultContextMenu = new VaultContextMenuStore();

export type { AllowedSurfaceIcon };
