import type { GraphemeCompactModuleOp, GraphemeModuleSummary } from "$lib/types/grapheme";

const MODULE_JOBS: Record<string, { label: string; blurb: string }> = {
  core: { label: "Utilities", blurb: "Messages, picking fields, everyday helpers" },
  web: { label: "Web", blurb: "Search the web and fetch pages" },
  html: { label: "HTML", blurb: "Parse and convert HTML" },
  json: { label: "JSON", blurb: "Read and write JSON data" },
  csv: { label: "Spreadsheets", blurb: "CSV and tabular data" },
  yaml: { label: "Config", blurb: "YAML config and structured text" },
  docs: { label: "Documents", blurb: "Documents and text files" },
  io: { label: "Files", blurb: "Files in and out" },
  shell: { label: "Shell", blurb: "Sandboxed OS commands" },
  email: { label: "Email", blurb: "Send and work with email" },
  data: { label: "Data", blurb: "Shape and transform data" },
  medousa: { label: "Medousa", blurb: "Digest, synthesize, and deliver results" },
};

/** Preferred defaults when no last-used module is stored. */
export const MODULE_DEFAULT_PREFERENCE = [
  "email",
  "csv",
  "web",
  "docs",
  "data",
  "io",
  "json",
  "html",
  "core",
] as const;

const LAST_MODULE_KEY = "medousa.lme.lastModuleId";

export function moduleJobLabel(moduleId: string): string {
  return MODULE_JOBS[moduleId]?.label ?? moduleId;
}

/** echo → Echo · pack_state_data → Pack State Data */
export function opHumanTitle(op: string): string {
  return op
    .split(/[._]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export function opMarkGlyph(op: string): string {
  const cleaned = op.replace(/[^a-zA-Z]/g, "");
  return (cleaned.charAt(0) || "?").toUpperCase();
}

export function moduleBlurb(entry: GraphemeModuleSummary | string): string {
  const id = typeof entry === "string" ? entry : entry.module_id;
  const known = MODULE_JOBS[id];
  if (known) return known.blurb;
  if (typeof entry !== "string") {
    return `${entry.op_count} actions you can insert`;
  }
  return "Actions you can insert";
}

export function moduleMatchesSearch(
  entry: GraphemeModuleSummary,
  needle: string,
): boolean {
  const q = needle.trim().toLowerCase();
  if (!q) return true;
  const label = moduleJobLabel(entry.module_id).toLowerCase();
  const blurb = moduleBlurb(entry).toLowerCase();
  return (
    entry.module_id.toLowerCase().includes(q) ||
    label.includes(q) ||
    blurb.includes(q) ||
    entry.effects.some((effect) => effect.toLowerCase().includes(q))
  );
}

export function loadLastModuleId(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const value = localStorage.getItem(LAST_MODULE_KEY)?.trim();
    return value || null;
  } catch {
    return null;
  }
}

export function saveLastModuleId(moduleId: string) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(LAST_MODULE_KEY, moduleId);
  } catch {
    // ignore quota / private mode
  }
}

export function pickDefaultModuleId(modules: GraphemeModuleSummary[]): string | null {
  if (modules.length === 0) return null;
  const ids = new Set(modules.map((entry) => entry.module_id));
  const last = loadLastModuleId();
  if (last && ids.has(last)) return last;
  for (const preferred of MODULE_DEFAULT_PREFERENCE) {
    if (ids.has(preferred)) return preferred;
  }
  return modules[0]!.module_id;
}

/** Common modules for the editor library picker (last-used first, then preference order). */
export function listCommonModules(
  modules: GraphemeModuleSummary[],
  limit = 6,
): GraphemeModuleSummary[] {
  if (modules.length === 0) return [];
  const byId = new Map(modules.map((entry) => [entry.module_id, entry]));
  const ordered: GraphemeModuleSummary[] = [];
  const seen = new Set<string>();
  const push = (id: string) => {
    if (seen.has(id)) return;
    const entry = byId.get(id);
    if (!entry) return;
    seen.add(id);
    ordered.push(entry);
  };
  const last = loadLastModuleId();
  if (last) push(last);
  for (const id of MODULE_DEFAULT_PREFERENCE) push(id);
  for (const entry of modules) {
    if (ordered.length >= limit) break;
    push(entry.module_id);
  }
  return ordered.slice(0, limit);
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
