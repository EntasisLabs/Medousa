export type ShellContextTarget =
  | { kind: "tab"; tabId: string; groupId: string; title: string }
  | { kind: "pane"; groupId: string };

export class ShellContextMenuStore {
  open = $state(false);
  x = $state(0);
  y = $state(0);
  target = $state<ShellContextTarget | null>(null);
  /** Inline picker for move-to-workspace. */
  pickingDesktop = $state(false);

  showAt(clientX: number, clientY: number, target: ShellContextTarget) {
    this.x = clientX;
    this.y = clientY;
    this.target = target;
    this.pickingDesktop = false;
    this.open = true;
  }

  showTab(clientX: number, clientY: number, tabId: string, groupId: string, title: string) {
    this.showAt(clientX, clientY, {
      kind: "tab",
      tabId,
      groupId,
      title: title.trim() || "Tab",
    });
  }

  showPane(clientX: number, clientY: number, groupId: string) {
    this.showAt(clientX, clientY, { kind: "pane", groupId });
  }

  pickDesktop() {
    this.pickingDesktop = true;
  }

  close() {
    this.open = false;
    this.target = null;
    this.pickingDesktop = false;
  }
}

export const shellContextMenu = new ShellContextMenuStore();
