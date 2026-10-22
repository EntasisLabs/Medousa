import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";
export type CreativeRailPreferences = {
  sort: "recent" | "name";
  pinnedOpen: boolean;
  recentOpen: boolean;
  scrollTop: number;
};
const key = (scope: string) => workshopScopedStorageKey("medousa:create-rail:v1", scope);
export function loadCreativeRailPreferences(scope: string): CreativeRailPreferences {
  const defaults: CreativeRailPreferences = { sort: "recent", pinnedOpen: true, recentOpen: true, scrollTop: 0 };
  try {
    const value = JSON.parse(localStorage.getItem(key(scope)) ?? "null");
    if (value?.sort === "name") defaults.sort = "name";
    for (const name of ["pinnedOpen", "recentOpen"] as const)
      if (typeof value?.[name] === "boolean") defaults[name] = value[name];
    if (Number.isFinite(value?.scrollTop) && value.scrollTop >= 0) defaults.scrollTop = value.scrollTop;
  } catch { /* Local preferences never prevent opening projects. */ }
  return defaults;
}
export function saveCreativeRailPreferences(scope: string, value: CreativeRailPreferences) {
  try { localStorage.setItem(key(scope), JSON.stringify(value)); } catch { /* Storage can be unavailable. */ }
}
