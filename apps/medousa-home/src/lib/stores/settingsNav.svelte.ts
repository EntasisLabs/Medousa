import { SETTINGS_SECTIONS, type SettingsSectionId } from "$lib/types/settings";
import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";

const LAST_SECTION_KEY = "medousa-home-settings-last-section";
const SECTION_IDS = new Set(SETTINGS_SECTIONS.map((section) => section.id));

function migrateSectionId(raw: string): SettingsSectionId | null {
  // Room + Rhythm merged into Preferences.
  if (raw === "room" || raw === "rhythm") return "preferences";
  // Memory + Models + Voice merged into Medousa Agent.
  if (raw === "memory" || raw === "models" || raw === "voice") return "agent";
  if (!SECTION_IDS.has(raw as SettingsSectionId)) return null;
  return raw as SettingsSectionId;
}

function readStoredSection(): SettingsSectionId | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(LAST_SECTION_KEY)?.trim();
    if (!raw) return null;
    return migrateSectionId(raw);
  } catch {
    return null;
  }
}

function writeStoredSection(section: SettingsSectionId) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(LAST_SECTION_KEY, section);
  } catch {
    /* ignore */
  }
}

function confirmDiscardUnsaved(): boolean {
  if (!workshopDefaults.dirty) return true;
  if (typeof window === "undefined") return true;
  return window.confirm("You have unsaved settings changes. Leave without saving?");
}

/** Settings section selection + jump-from-elsewhere. */
export class SettingsNavStore {
  activeSection = $state<SettingsSectionId>(readStoredSection() ?? "preferences");
  pendingSection = $state<SettingsSectionId | null>(null);

  openSection(section: SettingsSectionId) {
    if (section !== this.activeSection && !confirmDiscardUnsaved()) return;
    this.pendingSection = section;
    this.activeSection = section;
    writeStoredSection(section);
  }

  setActiveSection(section: SettingsSectionId) {
    if (section !== this.activeSection && !confirmDiscardUnsaved()) return;
    this.activeSection = section;
    writeStoredSection(section);
  }

  takePending(): SettingsSectionId | null {
    const section = this.pendingSection;
    this.pendingSection = null;
    if (section) {
      this.activeSection = section;
      writeStoredSection(section);
    }
    return section;
  }
}

export const settingsNav = new SettingsNavStore();
