import {
  COLOR_THEME_OPTIONS,
  DEFAULT_COLOR_THEME,
  isColorThemeId,
  type ColorThemeId,
} from "$lib/types/colorThemes";
import { resolveSkeletonThemeName } from "$lib/types/themeResolve";
import { loadTuiDefaults, persistTuiDefaults } from "$lib/config";
import { getRuntimeDefaults } from "$lib/daemon";
import { isTauri, isTauriMobilePlatform } from "$lib/platform";

const DARK_MODE_KEY = "medousa-home-dark-mode";
const COLOR_THEME_KEY = "medousa-home-color-theme";
const NOTIFICATIONS_KEY = "medousa-home-notifications";
const LIVE_ACTIVITY_KEY = "medousa-home-live-activity";
const REMOTE_PUSH_KEY = "medousa-home-remote-push";
const TECHNICAL_ACTIVITY_KEY = "medousa-home-technical-activity";
const WORKSHOP_GUIDANCE_KEY = "medousa-home-workshop-guidance";
const ENGINE_DETAILS_KEY = "medousa-home-engine-details-chat";
const AUTO_OPEN_WEB_KEY = "medousa-home-auto-open-web-on-agent";
const WORK_HIDE_HOURS_KEY = "medousa-home-work-hide-hours";
const WORK_WIPE_DAYS_KEY = "medousa-home-work-wipe-days";

const DEFAULT_WORK_HIDE_HOURS = 24;
const DEFAULT_WORK_WIPE_DAYS = 7;

export class SettingsStore {
  darkMode = $state(loadDarkMode());
  colorTheme = $state(loadColorTheme());
  notificationsEnabled = $state(loadNotifications());
  liveActivityEnabled = $state(loadLiveActivity());
  remotePushEnabled = $state(loadRemotePush());
  showTechnicalActivity = $state(loadTechnicalActivity());
  /** Journey steps, recipe cards, and friendly run summaries in Workshop / Automations. */
  showWorkshopGuidance = $state(loadWorkshopGuidance());
  /** Show orchestrator/fallback/tool telemetry in chat (hidden by default; never deleted from history). */
  showEngineDetailsInChat = $state(loadEngineDetailsInChat());
  /** When agent browses, switch to Web surface automatically (inverse: stay on Chat). */
  autoOpenWebOnAgentBrowse = $state(loadAutoOpenWebOnAgentBrowse());
  workCardHideAfterHours = $state(loadWorkCardHideAfterHours());
  workCardWipeAfterDays = $state(loadWorkCardWipeAfterDays());
  diagnosticsOpen = $state(false);
  daemonUrl = $state("");
  daemonMessage = $state<string | null>(null);
  savingDaemon = $state(false);

  applyTheme() {
    if (typeof document === "undefined") return;
    document.documentElement.classList.toggle("dark", this.darkMode);
    const skeletonTheme = resolveSkeletonThemeName(this.colorTheme, this.darkMode);
    document.documentElement.dataset.theme = skeletonTheme;
    document.body.dataset.theme = skeletonTheme;
  }

  setDarkMode(enabled: boolean) {
    this.darkMode = enabled;
    localStorage.setItem(DARK_MODE_KEY, enabled ? "1" : "0");
    this.applyTheme();
  }

  setColorTheme(theme: ColorThemeId, options?: { persistWorkshop?: boolean }) {
    this.colorTheme = theme;
    localStorage.setItem(COLOR_THEME_KEY, theme);
    this.applyTheme();
    if (options?.persistWorkshop === false) return;
    if (isTauri()) {
      void import("$lib/stores/workshops.svelte").then(({ workshops }) =>
        workshops.saveColorTheme(theme),
      );
    }
  }

  setNotificationsEnabled(enabled: boolean) {
    this.notificationsEnabled = enabled;
    localStorage.setItem(NOTIFICATIONS_KEY, enabled ? "1" : "0");
  }

  setLiveActivityEnabled(enabled: boolean) {
    this.liveActivityEnabled = enabled;
    localStorage.setItem(LIVE_ACTIVITY_KEY, enabled ? "1" : "0");
    if (isTauriMobilePlatform()) {
      void import("$lib/liveActivity").then(({ resetLiveActivitySync, syncLiveActivity, buildLiveActivityPayload }) => {
        resetLiveActivitySync();
        if (!enabled) {
          void syncLiveActivity({
            mood: "quiet",
            workshopName: "",
            eyebrow: "Quiet",
            headline: "Nothing needs you",
            blockedCount: 0,
          });
        }
      });
    }
  }

  setRemotePushEnabled(enabled: boolean) {
    this.remotePushEnabled = enabled;
    localStorage.setItem(REMOTE_PUSH_KEY, enabled ? "1" : "0");
    if (isTauriMobilePlatform()) {
      void import("$lib/pushRegistration").then(({ setRemotePushEnabled }) => {
        setRemotePushEnabled(enabled);
      });
    }
  }

  setShowTechnicalActivity(enabled: boolean) {
    this.showTechnicalActivity = enabled;
    localStorage.setItem(TECHNICAL_ACTIVITY_KEY, enabled ? "1" : "0");
  }

  setShowWorkshopGuidance(enabled: boolean) {
    this.showWorkshopGuidance = enabled;
    localStorage.setItem(WORKSHOP_GUIDANCE_KEY, enabled ? "1" : "0");
    if (enabled) {
      void import("$lib/config/workshopGuidance").then(({ resetWorkshopJourneyDismissed }) =>
        resetWorkshopJourneyDismissed(),
      );
    }
  }

  setShowEngineDetailsInChat(enabled: boolean) {
    this.showEngineDetailsInChat = enabled;
    localStorage.setItem(ENGINE_DETAILS_KEY, enabled ? "1" : "0");
  }

  setAutoOpenWebOnAgentBrowse(enabled: boolean) {
    this.autoOpenWebOnAgentBrowse = enabled;
    localStorage.setItem(AUTO_OPEN_WEB_KEY, enabled ? "1" : "0");
  }

  setWorkCardHideAfterHours(hours: number) {
    const normalized = clampWorkHideHours(hours);
    this.workCardHideAfterHours = normalized;
    localStorage.setItem(WORK_HIDE_HOURS_KEY, String(normalized));
  }

  setWorkCardWipeAfterDays(days: number) {
    const normalized = clampWorkWipeDays(days);
    this.workCardWipeAfterDays = normalized;
    localStorage.setItem(WORK_WIPE_DAYS_KEY, String(normalized));
  }

  /** Pull authoritative retention policy from the Mac daemon (`tui_defaults.json`). */
  async hydrateWorkRetentionFromDaemon() {
    try {
      const defaults = await getRuntimeDefaults();
      this.setWorkCardHideAfterHours(defaults.work_card_hide_after_hours);
      this.setWorkCardWipeAfterDays(defaults.work_card_wipe_after_days);
    } catch {
      // Offline — keep local fallback until connected.
    }
  }

  /** Persist retention fields into `tui_defaults.json` (Mac desktop only). */
  async persistWorkRetention() {
    if (!isTauri() || isTauriMobilePlatform()) return;
    const current = await loadTuiDefaults();
    await persistTuiDefaults({
      ...current,
      workCardHideAfterHours: this.workCardHideAfterHours,
      workCardWipeAfterDays: this.workCardWipeAfterDays,
    });
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

function loadWorkshopGuidance(): boolean {
  if (typeof localStorage === "undefined") return true;
  return localStorage.getItem(WORKSHOP_GUIDANCE_KEY) !== "0";
}

function loadEngineDetailsInChat(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(ENGINE_DETAILS_KEY) === "1";
}

function loadAutoOpenWebOnAgentBrowse(): boolean {
  if (typeof localStorage === "undefined") return true;
  return localStorage.getItem(AUTO_OPEN_WEB_KEY) !== "0";
}

function loadWorkCardHideAfterHours(): number {
  if (typeof localStorage === "undefined") return DEFAULT_WORK_HIDE_HOURS;
  return clampWorkHideHours(Number(localStorage.getItem(WORK_HIDE_HOURS_KEY)));
}

function loadWorkCardWipeAfterDays(): number {
  if (typeof localStorage === "undefined") return DEFAULT_WORK_WIPE_DAYS;
  return clampWorkWipeDays(Number(localStorage.getItem(WORK_WIPE_DAYS_KEY)));
}

function clampWorkHideHours(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_WORK_HIDE_HOURS;
  return Math.min(168, Math.max(1, Math.round(value)));
}

function clampWorkWipeDays(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_WORK_WIPE_DAYS;
  return Math.min(90, Math.max(1, Math.round(value)));
}

function loadNotifications(): boolean {
  if (typeof localStorage === "undefined") return true;
  const stored = localStorage.getItem(NOTIFICATIONS_KEY);
  if (stored === "0") return false;
  return true;
}

function loadLiveActivity(): boolean {
  if (typeof localStorage === "undefined") return true;
  const stored = localStorage.getItem(LIVE_ACTIVITY_KEY);
  if (stored === "0") return false;
  return true;
}

function loadRemotePush(): boolean {
  if (typeof localStorage === "undefined") return true;
  const stored = localStorage.getItem(REMOTE_PUSH_KEY);
  if (stored === "0") return false;
  return true;
}

export { COLOR_THEME_OPTIONS };
export const settings = new SettingsStore();
