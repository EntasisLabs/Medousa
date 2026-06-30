export type CommandSpotlightMode = "default" | "notes";

export interface CommandPromptStep {
  commandId: string;
  label: string;
  placeholder: string;
  submitLabel: string;
}

class CommandSpotlightStore {
  open = $state(false);
  mode = $state<CommandSpotlightMode>("default");
  promptStep = $state<CommandPromptStep | null>(null);
  /** Command waiting for prompt input (not always in filtered list). */
  pendingCommand = $state<import("$lib/commands/types").WorkshopCommand | null>(null);

  openSpotlight(mode: CommandSpotlightMode = "default") {
    this.mode = mode;
    this.promptStep = null;
    this.pendingCommand = null;
    this.open = true;
  }

  openNotes() {
    this.openSpotlight("notes");
  }

  closeSpotlight() {
    this.open = false;
    this.promptStep = null;
    this.pendingCommand = null;
    this.mode = "default";
  }

  toggleSpotlight() {
    if (this.open) {
      this.closeSpotlight();
    } else {
      this.openSpotlight();
    }
  }

  beginPrompt(
    step: CommandPromptStep,
    command: import("$lib/commands/types").WorkshopCommand,
  ) {
    this.promptStep = step;
    this.pendingCommand = command;
  }

  cancelPrompt() {
    this.promptStep = null;
    this.pendingCommand = null;
  }
}

export const commandSpotlight = new CommandSpotlightStore();

/** @deprecated use commandSpotlight — kept for vault editor shim */
export const vaultQuickSwitcher = {
  openSwitcher() {
    commandSpotlight.openNotes();
  },
  closeSwitcher() {
    commandSpotlight.closeSpotlight();
  },
  toggle() {
    if (commandSpotlight.open && commandSpotlight.mode === "notes") {
      commandSpotlight.closeSpotlight();
    } else {
      commandSpotlight.openNotes();
    }
  },
};
