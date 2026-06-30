const STORAGE_KEY = "medousa-home-command-usage";

type UsageMap = Record<string, number>;

function readMap(): UsageMap {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as UsageMap;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeMap(map: UsageMap) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
}

export function recordCommandUsage(commandId: string) {
  const map = readMap();
  map[commandId] = (map[commandId] ?? 0) + 1;
  writeMap(map);
}

export function commandUsageCount(commandId: string): number {
  return readMap()[commandId] ?? 0;
}

export function usageScoreBoost(commandId: string): number {
  const count = commandUsageCount(commandId);
  if (count <= 0) return 0;
  return Math.min(40, Math.log2(count + 1) * 12);
}
