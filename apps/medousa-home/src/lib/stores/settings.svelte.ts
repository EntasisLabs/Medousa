import {
  COLOR_THEME_OPTIONS,
  DEFAULT_COLOR_THEME,
  isColorThemeId,
  type ColorThemeId,
} from "$lib/types/colorThemes";

const DARK_MODE_KEY = "medousa-home-dark-mode";
const COLOR_THEME_KEY = "medousa-home-color-theme";
const NOTIFICATIONS_KEY = "medousa-home-notifications";
const TECHNICAL_ACTIVITY_KEY = "medousa-home-technical-activity";

export class SettingsStore {
  darkMode = $state(loadDarkMode());
  colorTheme = $state(loadColorTheme());
  notificationsEnabled = $state(loadNotifications());
  showTechnicalActivity = $state(loadTechnicalActivity());
  diagnosticsOpen = $state(false);
  daemonUrl = $state("");
  daemonMessage = $state<string | null>(null);
  savingDaemon = $state(false);

  applyTheme() {
    if (typeof document === "undefined") return;
    document.documentElement.classList.toggle("dark", this.darkMode);
    const theme = this.darkMode ? this.colorTheme : DEFAULT_COLOR_THEME;
    document.documentElement.dataset.theme = theme;
    document.body.dataset.theme = theme;
  }

  setDarkMode(enabled: boolean) {
    this.darkMode = enabled;
    localStorage.setItem(DARK_MODE_KEY, enabled ? "1" : "0");
    this.applyTheme();
  }

  setColorTheme(theme: ColorThemeId) {
    this.colorTheme = theme;
    localStorage.setItem(COLOR_THEME_KEY, theme);
    this.applyTheme();
  }

  setNotificationsEnabled(enabled: boolean) {
    this.notificationsEnabled = enabled;
    localStorage.setItem(NOTIFICATIONS_KEY, enabled ? "1" : "0");
  }

  setShowTechnicalActivity(enabled: boolean) {
    this.showTechnicalActivity = enabled;
    localStorage.setItem(TECHNICAL_ACTIVITY_KEY, enabled ? "1" : "0");
  }
}

function loadDarkMode(): boolean {
  if (typeof localStorage === "undefined") return true;
  const stored = localStorage.getItem(DARK_MODE_KEY);
  if (stored === "0") return false;
  return true;
}

function loadColorTheme(): ColorThemeId {
  if (typeof localStorage === "undefined") return DEFAULT_COLOR_THEME;
  const stored = localStorage.getItem(COLOR_THEME_KEY);
  return isColorThemeId(stored) ? stored : DEFAULT_COLOR_THEME;
}

function loadTechnicalActivity(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(TECHNICAL_ACTIVITY_KEY) === "1";
}

function loadNotifications(): boolean {
  if (typeof localStorage === "undefined") return true;
  const stored = localStorage.getItem(NOTIFICATIONS_KEY);
  if (stored === "0") return false;
  return true;
}

export { COLOR_THEME_OPTIONS };
export const settings = new SettingsStore();
