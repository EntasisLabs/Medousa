const ACTIVITY_WIDTH_KEY = "medousa-home-activity-width";
const VAULT_TREE_WIDTH_KEY = "medousa-home-vault-tree-width";
const WORK_INSPECTOR_WIDTH_KEY = "medousa-home-work-inspector-width";
const SESSION_DRAWER_KEY = "medousa-home-session-drawer";
const IDENTITY_DRAWER_KEY = "medousa-home-identity-drawer";
const ACTIVITY_COLLAPSED_KEY = "medousa-home-activity-collapsed";

export class LayoutStore {
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
