export type AutomationsSection = "scripts" | "schedules" | "flows" | "history";

export class AutomationsNavStore {
  pendingSection = $state<AutomationsSection | null>(null);

  openSection(section: AutomationsSection) {
    this.pendingSection = section;
  }

  consumeSection(): AutomationsSection | null {
    const section = this.pendingSection;
    this.pendingSection = null;
    return section;
  }
}

export const automationsNav = new AutomationsNavStore();
