export type MobileDrawTool = "ink" | "eraser" | "select" | "hand";

export type MobileDrawControlSet = {
  tool: MobileDrawTool;
  optionsOpen: boolean;
  canUndo: boolean;
  canRedo: boolean;
  setTool: (tool: MobileDrawTool) => void;
  openOptions: () => void;
  undo: () => void;
  redo: () => void;
};

type Registration = MobileDrawControlSet & { owner: object };

export class MobileDrawControls {
  current = $state.raw<Registration | null>(null);
  #registrations = new Map<object, MobileDrawControlSet>();

  register(owner: object, controls: MobileDrawControlSet) {
    this.#registrations.set(owner, controls);
    if (!this.current || this.current.owner === owner) {
      this.current = { owner, ...controls };
    }
  }

  activate(owner: object) {
    const controls = this.#registrations.get(owner);
    if (controls) this.current = { owner, ...controls };
  }

  unregister(owner: object) {
    this.#registrations.delete(owner);
    if (this.current?.owner !== owner) return;
    const fallback = [...this.#registrations.entries()].at(-1);
    this.current = fallback ? { owner: fallback[0], ...fallback[1] } : null;
  }
}

export const mobileDrawControls = new MobileDrawControls();
