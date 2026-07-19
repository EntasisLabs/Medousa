export type ScriptContextTarget = {
  scriptId: string;
  name: string;
};

export class ScriptContextMenuStore {
  open = $state(false);
  x = $state(0);
  y = $state(0);
  target = $state<ScriptContextTarget | null>(null);
  confirmDelete = $state(false);

  showAt(clientX: number, clientY: number, target: ScriptContextTarget) {
    this.x = clientX;
    this.y = clientY;
    this.target = target;
    this.confirmDelete = false;
    this.open = true;
  }

  askDelete() {
    this.confirmDelete = true;
  }

  close() {
    this.open = false;
    this.target = null;
    this.confirmDelete = false;
  }
}

export const scriptContextMenu = new ScriptContextMenuStore();
