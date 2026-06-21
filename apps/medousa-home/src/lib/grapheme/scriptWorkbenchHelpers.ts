import type { GraphemeCompactModuleOp, GraphemeModuleSummary } from "$lib/types/grapheme";

export function moduleBlurb(entry: GraphemeModuleSummary): string {
  const blurbs: Record<string, string> = {
    core: "Messages, picking fields, everyday utilities",
    web: "Search the web and fetch pages",
    html: "Parse and convert HTML",
    json: "Read and write JSON data",
    csv: "Spreadsheet-style data",
    yaml: "Config and structured text",
    docs: "Documents and text files",
    io: "Files in and out",
  };
  return blurbs[entry.module_id] ?? `${entry.op_count} ready-made actions you can insert`;
}

export function effectBadgeClass(effect: string): string {
  const normalized = String(effect).toLowerCase();
  if (normalized === "network" || normalized === "secrets") {
    return "scripts-workbench-effect-chip-warning";
  }
  if (normalized === "pure") {
    return "scripts-workbench-effect-chip-muted";
  }
  return "scripts-workbench-effect-chip-default";
}

export function stabilityLabel(op: GraphemeCompactModuleOp): string {
  return op.stability || "stable";
}
