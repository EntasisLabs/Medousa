export type AutomationsSection = "scripts" | "schedules" | "flows" | "history";

export class AutomationsNavStore {
  pendingSection = $state<AutomationsSection | null>(null);

  openSection(section: AutomationsSection) {
    this.pendingSection = section;
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
