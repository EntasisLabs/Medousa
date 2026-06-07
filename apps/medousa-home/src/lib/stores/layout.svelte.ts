const ACTIVITY_WIDTH_KEY = "medousa-home-activity-width";
const VAULT_TREE_WIDTH_KEY = "medousa-home-vault-tree-width";
const WORK_INSPECTOR_WIDTH_KEY = "medousa-home-work-inspector-width";

export class LayoutStore {
  activityWidth = $state(loadWidth(ACTIVITY_WIDTH_KEY, 288));
  vaultTreeWidth = $state(loadWidth(VAULT_TREE_WIDTH_KEY, 224));
  workInspectorWidth = $state(loadWidth(WORK_INSPECTOR_WIDTH_KEY, 360));

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
}

function loadWidth(key: string, fallback: number): number {
  if (typeof localStorage === "undefined") return fallback;
  const raw = Number(localStorage.getItem(key));
  return Number.isFinite(raw) ? clamp(raw, 180, 520) : fallback;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export const layout = new LayoutStore();
