const DARK_MODE_KEY = "medousa-home-dark-mode";
const NOTIFICATIONS_KEY = "medousa-home-notifications";
const TECHNICAL_ACTIVITY_KEY = "medousa-home-technical-activity";

export class SettingsStore {
  darkMode = $state(loadDarkMode());
  notificationsEnabled = $state(loadNotifications());
  showTechnicalActivity = $state(loadTechnicalActivity());
  diagnosticsOpen = $state(false);
  daemonUrl = $state("");
  daemonMessage = $state<string | null>(null);
  savingDaemon = $state(false);

  applyTheme() {
    if (typeof document === "undefined") return;
    document.documentElement.classList.toggle("dark", this.darkMode);
  }

  setDarkMode(enabled: boolean) {
    this.darkMode = enabled;
    localStorage.setItem(DARK_MODE_KEY, enabled ? "1" : "0");
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

export const settings = new SettingsStore();
