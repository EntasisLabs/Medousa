import type { SettingsSectionId } from "$lib/types/settings";

/** Jump to a specific Settings section when opening the panel. */
export class SettingsNavStore {
  pendingSection = $state<SettingsSectionId | null>(null);

  openSection(section: SettingsSectionId) {
    this.pendingSection = section;
  }

  takePending(): SettingsSectionId | null {
    const section = this.pendingSection;
    this.pendingSection = null;
    return section;
  }
}

export const settingsNav = new SettingsNavStore();
