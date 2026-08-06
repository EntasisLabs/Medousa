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

  /** Telescope-style resume: last query + mode before close. */
  lastQuery = $state("");
  lastMode = $state<CommandSpotlightMode>("default");
  resumeAvailable = $state(false);
  /** When set, CommandSpotlight hydrates the input once. */
  seedQuery = $state<string | null>(null);

  openSpotlight(mode: CommandSpotlightMode = "default") {
    this.mode = mode;
    this.promptStep = null;
    this.pendingCommand = null;
    this.seedQuery = null;
    this.open = true;
  }

  /** VS Code-style command palette — Spotlight seeded in `>` advanced mode. */
  openCommandPalette() {
    this.mode = "default";
    this.promptStep = null;
    this.pendingCommand = null;
    this.seedQuery = ">";
    this.open = true;
  }

  /** Reopen with previous query/mode (Telescope resume). */
  resumeSpotlight() {
    this.mode = this.lastMode;
    this.promptStep = null;
    this.pendingCommand = null;
    this.seedQuery = this.lastQuery;
    this.open = true;
  }

  /** Restore last query while already open. */
  restoreLastQuery() {
    this.seedQuery = this.lastQuery;
    this.mode = this.lastMode;
  }

  openNotes() {
    this.openSpotlight("notes");
  }

  rememberQuery(query: string, mode: CommandSpotlightMode) {
    this.lastQuery = query;
    this.lastMode = mode;
    this.resumeAvailable = query.trim().length > 0 || mode === "notes";
  }

  closeSpotlight() {
    this.open = false;
    this.promptStep = null;
    this.pendingCommand = null;
    this.mode = "default";
    this.seedQuery = null;
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
