const STORAGE_KEY = "medousa-home-command-usage";

type UsageMap = Record<string, number>;

let cachedMap: UsageMap | null = null;

function readMap(): UsageMap {
  if (cachedMap) return cachedMap;
  if (typeof localStorage === "undefined") {
    cachedMap = {};
    return cachedMap;
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      cachedMap = {};
      return cachedMap;
    }
    const parsed = JSON.parse(raw) as UsageMap;
    cachedMap = parsed && typeof parsed === "object" ? parsed : {};
    return cachedMap;
  } catch {
    cachedMap = {};
    return cachedMap;
  }
}

function writeMap(map: UsageMap) {
  cachedMap = map;
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
}

export function recordCommandUsage(commandId: string) {
  const map = { ...readMap() };
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
