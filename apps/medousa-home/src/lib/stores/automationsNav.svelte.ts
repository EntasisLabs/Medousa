export type AutomationsSection = "scripts" | "schedules" | "flows" | "history";

export type AutomationsChromeMode = "browse" | "flow-editor" | "script-editor";

export class AutomationsNavStore {
  pendingSection = $state<AutomationsSection | null>(null);
  /** Active section for mobile top chrome (session). */
  currentSection = $state<AutomationsSection>("scripts");
  /** Mobile top-chrome mode — panels write this so chrome stays in sync. */
  mobileChromeMode = $state<AutomationsChromeMode>("script-editor");

  setCurrentSection(section: AutomationsSection) {
    this.currentSection = section;
    if (section !== "scripts" && this.mobileChromeMode === "script-editor") {
      this.mobileChromeMode = "browse";
    }
    if (section === "scripts" && this.mobileChromeMode !== "flow-editor") {
      this.mobileChromeMode = "script-editor";
    }
  }

  setMobileChromeMode(mode: AutomationsChromeMode) {
    this.mobileChromeMode = mode;
  }

  openSection(section: AutomationsSection) {
    this.pendingSection = section;
    this.setCurrentSection(section);
    void import("$lib/stores/lmeWorkspace.svelte").then(({ lmeWorkspace }) => {
      lmeWorkspace.openAutomationsSection(section);
    });
    void import("$lib/stores/layout.svelte").then(({ layout }) => {
      layout.navigateDesktop("library", { bump: true });
    });
  }

  consumeSection(): AutomationsSection | null {
    const section = this.pendingSection;
    this.pendingSection = null;
    return section;
  }
}

export const automationsNav = new AutomationsNavStore();
