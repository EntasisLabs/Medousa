import type { SettingsSectionId } from "$lib/types/settings";

/** Settings section selection + jump-from-elsewhere. */
export class SettingsNavStore {
  activeSection = $state<SettingsSectionId>("room");
  pendingSection = $state<SettingsSectionId | null>(null);

  openSection(section: SettingsSectionId) {
    this.pendingSection = section;
    this.activeSection = section;
  }

  setActiveSection(section: SettingsSectionId) {
    this.activeSection = section;
  }

  takePending(): SettingsSectionId | null {
    const section = this.pendingSection;
    this.pendingSection = null;
    if (section) this.activeSection = section;
    return section;
  }
}

export const settingsNav = new SettingsNavStore();
