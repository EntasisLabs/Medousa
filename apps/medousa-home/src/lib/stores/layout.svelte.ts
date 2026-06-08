import type { MobileTab, YouDestination } from "$lib/types/mobile";
import { shouldUseMobileShell } from "$lib/platform";

const ACTIVITY_WIDTH_KEY = "medousa-home-activity-width";
const VAULT_TREE_WIDTH_KEY = "medousa-home-vault-tree-width";
const WORK_INSPECTOR_WIDTH_KEY = "medousa-home-work-inspector-width";
const SESSION_DRAWER_KEY = "medousa-home-session-drawer";
const IDENTITY_DRAWER_KEY = "medousa-home-identity-drawer";
const ACTIVITY_COLLAPSED_KEY = "medousa-home-activity-collapsed";
const MOBILE_TAB_KEY = "medousa-home-mobile-tab";

export class LayoutStore {
  isMobile = $state(
    typeof window !== "undefined" ? shouldUseMobileShell() : false,
  );
  mobileTab = $state<MobileTab>(loadMobileTab());
  youDestination = $state<YouDestination>("hub");
  activitySheetOpen = $state(false);
  askSheetOpen = $state(false);
  activityWidth = $state(loadWidth(ACTIVITY_WIDTH_KEY, 288));
  vaultTreeWidth = $state(loadWidth(VAULT_TREE_WIDTH_KEY, 224));
  workInspectorWidth = $state(loadWidth(WORK_INSPECTOR_WIDTH_KEY, 360));
  sessionDrawerOpen = $state(loadFlag(SESSION_DRAWER_KEY, false));
  identityDrawerOpen = $state(loadFlag(IDENTITY_DRAWER_KEY, false));
  activityCollapsed = $state(loadFlag(ACTIVITY_COLLAPSED_KEY, false));

  setActivityWidth(width: number) {
    this.activityWidth = clamp(width, 220, 520);
    localStorage.setItem(ACTIVITY_WIDTH_KEY, String(this.activityWidth));
  }

  setVaultTreeWidth(width: number) {
    this.vaultTreeWidth = clamp(width, 180, 420);
    localStorage.setItem(VAULT_TREE_WIDTH_KEY, String(this.vaultTreeWidth));
  }

  setWorkInspectorWidth(width: number) {
    this.workInspectorWidth = clamp(width, 280, 560);
    localStorage.setItem(WORK_INSPECTOR_WIDTH_KEY, String(this.workInspectorWidth));
  }

  setSessionDrawerOpen(open: boolean) {
    this.sessionDrawerOpen = open;
    localStorage.setItem(SESSION_DRAWER_KEY, open ? "1" : "0");
  }

  toggleSessionDrawer() {
    this.setSessionDrawerOpen(!this.sessionDrawerOpen);
  }

  setIdentityDrawerOpen(open: boolean) {
    this.identityDrawerOpen = open;
    localStorage.setItem(IDENTITY_DRAWER_KEY, open ? "1" : "0");
  }

  toggleIdentityDrawer() {
    this.setIdentityDrawerOpen(!this.identityDrawerOpen);
  }

  setActivityCollapsed(collapsed: boolean) {
    this.activityCollapsed = collapsed;
    localStorage.setItem(ACTIVITY_COLLAPSED_KEY, collapsed ? "1" : "0");
  }

  toggleActivityCollapsed() {
    this.setActivityCollapsed(!this.activityCollapsed);
  }

  setMobile(mobile: boolean) {
    this.isMobile = mobile;
  }

  setMobileTab(tab: MobileTab) {
    this.mobileTab = tab;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(MOBILE_TAB_KEY, tab);
    }
  }

  openYou(destination: YouDestination) {
    this.youDestination = destination;
    this.mobileTab = "you";
  }

  backToYouHub() {
    this.youDestination = "hub";
  }

  setActivitySheetOpen(open: boolean) {
    this.activitySheetOpen = open;
  }

  toggleActivitySheet() {
    this.setActivitySheetOpen(!this.activitySheetOpen);
  }

  setAskSheetOpen(open: boolean) {
    this.askSheetOpen = open;
  }

  openAskSheet() {
    this.askSheetOpen = true;
  }
}

function loadMobileTab(): MobileTab {
  if (typeof localStorage === "undefined") return "pulse";
  const stored = localStorage.getItem(MOBILE_TAB_KEY);
  if (stored === "pulse" || stored === "work" || stored === "chat" || stored === "you") {
    return stored;
  }
  return "pulse";
}

function loadWidth(key: string, fallback: number): number {
  if (typeof localStorage === "undefined") return fallback;
  const raw = Number(localStorage.getItem(key));
  return Number.isFinite(raw) ? clamp(raw, 180, 520) : fallback;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function loadFlag(key: string, fallback: boolean): boolean {
  if (typeof localStorage === "undefined") return fallback;
  const stored = localStorage.getItem(key);
  if (stored === "1") return true;
  if (stored === "0") return false;
  return fallback;
}

export const layout = new LayoutStore();
