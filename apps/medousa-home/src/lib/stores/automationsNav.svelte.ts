export type AutomationsSection =
  | "scripts"
  | "agents"
  | "flows"
  | "schedules"
  | "history";

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
    this.pendingSection = section === "history" || section === "agents" ? null : section;
    this.setCurrentSection(section);
    if (section === "history") {
      void import("$lib/stores/commandSpotlight.svelte").then(({ commandSpotlight }) => commandSpotlight.openActivity());
      return;
    }
    void Promise.all([
      import("$lib/stores/lmeWorkspace.svelte"),
      import("$lib/runtime/layout.svelte"),
      import("$lib/stores/shellTabs.svelte"),
    ]).then(([{ lmeWorkspace }, { layout }, { shellTabs }]) => {
      lmeWorkspace.openAutomationsSection(section);
      if (layout.isMobile) layout.openMore(section === "agents" ? "workshop" : "automations");
      else if (section === "agents") {
        shellTabs.enterLmeFamily("library");
        layout.openShellSidebarView("workshop");
      } else {
        shellTabs.enterLmeFamily("library");
        layout.openShellSidebarView("automations");
      }
    });
  }

  consumeSection(): AutomationsSection | null {
    const section = this.pendingSection;
    this.pendingSection = null;
    return section;
  }
}

export const automationsNav = new AutomationsNavStore();
