import type { CapabilityListEntry } from "$lib/types/catalog";

export type ToolFilterChip = "all" | "mcp" | "grapheme";

export const TOOL_FILTER_CHIPS: { id: ToolFilterChip; label: string }[] = [
  { id: "all", label: "All" },
  { id: "grapheme", label: "Grapheme" },
  { id: "mcp", label: "MCP" },
];

export function matchesToolFilter(
  entry: CapabilityListEntry,
  filter: ToolFilterChip,
): boolean {
  switch (filter) {
    case "mcp":
      return entry.has_mcp ?? bindingsFor(entry).some((b) => b.source === "mcp");
    case "grapheme":
      return (
        entry.has_grapheme ??
        bindingsFor(entry).some((b) => b.source === "grapheme")
      );
    default:
      return true;
  }
}

export function filterTools(
  entries: CapabilityListEntry[],
  query: string,
  filter: ToolFilterChip,
): CapabilityListEntry[] {
  const normalized = query.trim().toLowerCase();
  return entries.filter((entry) => {
    if (!matchesToolFilter(entry, filter)) return false;
    if (!normalized) return true;
    const haystack = [
      entry.title,
      entry.id,
      entry.description ?? "",
      entry.domain ?? "",
      ...bindingsFor(entry).map((binding) => binding.reference),
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(normalized);
  });
}

export function toolDomain(entry: CapabilityListEntry): string {
  return (
    entry.domain ||
    entry.id.split("_")[0]?.toUpperCase() ||
    "OTHER"
  );
}

export function bindingsFor(entry: CapabilityListEntry) {
  return entry.bindings_summary ?? [];
}

export function groupTools(
  entries: CapabilityListEntry[],
): { label: string; entries: CapabilityListEntry[] }[] {
  const buckets = new Map<string, CapabilityListEntry[]>();
  for (const entry of entries) {
    const label = toolDomain(entry);
    const group = buckets.get(label) ?? [];
    group.push(entry);
    buckets.set(label, group);
  }

  return [...buckets.keys()]
    .sort((left, right) => left.localeCompare(right))
    .map((label) => ({
      label,
      entries: [...(buckets.get(label) ?? [])].sort((left, right) =>
        left.title.localeCompare(right.title),
      ),
    }));
}

export function bindingSourcesLabel(entry: CapabilityListEntry): string {
  const parts: string[] = [];
  const summary = bindingsFor(entry);
  const graphemeCount = summary.filter(
    (binding) => binding.source === "grapheme",
  ).length;
  const mcpCount = summary.filter((binding) => binding.source === "mcp").length;
  if (graphemeCount > 0) {
    parts.push(`Grapheme ${graphemeCount}`);
  }
  if (mcpCount > 0) {
    parts.push(`MCP ${mcpCount}`);
  }
  return parts.join(" · ") || "No bindings";
}

export function primaryEffectClass(entry: CapabilityListEntry): string | null {
  for (const binding of bindingsFor(entry)) {
    if (binding.effect_class) return binding.effect_class;
  }
  return null;
}
