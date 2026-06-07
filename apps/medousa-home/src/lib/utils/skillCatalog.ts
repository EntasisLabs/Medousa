import type { ManuscriptCatalogEntry } from "$lib/types/catalog";

export type SkillFilterChip = "all" | "runnable" | "sandbox" | "imported";

export const SKILL_FILTER_CHIPS: { id: SkillFilterChip; label: string }[] = [
  { id: "all", label: "All" },
  { id: "runnable", label: "Runnable" },
  { id: "sandbox", label: "Sandbox" },
  { id: "imported", label: "Imported" },
];

export function matchesSkillFilter(
  entry: ManuscriptCatalogEntry,
  filter: SkillFilterChip,
): boolean {
  switch (filter) {
    case "runnable":
      return entry.has_scripts;
    case "sandbox":
      return entry.openshell_enabled;
    case "imported":
      return entry.scope === "user";
    default:
      return true;
  }
}

export function skillCategoryLabel(entry: ManuscriptCatalogEntry): string {
  const slash = entry.id.indexOf("/");
  if (slash > 0) {
    return entry.id
      .slice(0, slash)
      .replace(/-/g, " ")
      .toUpperCase();
  }
  if (entry.scope === "project") return "Project";
  if (entry.scope === "user") return "Imported";
  return entry.scope;
}

export function groupSkills(
  entries: ManuscriptCatalogEntry[],
): { label: string; entries: ManuscriptCatalogEntry[] }[] {
  const buckets = new Map<string, ManuscriptCatalogEntry[]>();
  for (const entry of entries) {
    const label = skillCategoryLabel(entry);
    const group = buckets.get(label) ?? [];
    group.push(entry);
    buckets.set(label, group);
  }

  const order = ["Project", "Imported"];
  const labels = [...buckets.keys()].sort((left, right) => {
    const leftIdx = order.indexOf(left);
    const rightIdx = order.indexOf(right);
    if (leftIdx >= 0 || rightIdx >= 0) {
      return (leftIdx < 0 ? 99 : leftIdx) - (rightIdx < 0 ? 99 : rightIdx);
    }
    return left.localeCompare(right);
  });

  return labels.map((label) => ({
    label,
    entries: [...(buckets.get(label) ?? [])].sort((left, right) =>
      left.name.localeCompare(right.name),
    ),
  }));
}

export function filterSkills(
  entries: ManuscriptCatalogEntry[],
  query: string,
  filter: SkillFilterChip,
): ManuscriptCatalogEntry[] {
  const normalized = query.trim().toLowerCase();
  return entries.filter((entry) => {
    if (!matchesSkillFilter(entry, filter)) return false;
    if (!normalized) return true;
    const haystack = [
      entry.name,
      entry.id,
      entry.description ?? "",
      entry.scope,
      entry.path,
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(normalized);
  });
}
