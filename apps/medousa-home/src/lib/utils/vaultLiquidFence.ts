/**
 * Liquid fence extract / parse / serialize for Live Configure
 * (card, callout, dashboard, tabs, steps, accordion, code, tree, compare).
 */

import { findFirstPipeTable, serializePipeTable } from "$lib/utils/markdownTable";

export type LiquidFenceLang =
  | "card"
  | "callout"
  | "dashboard"
  | "tabs"
  | "steps"
  | "accordion"
  | "code"
  | "tree"
  | "compare";

export interface LiquidFenceBlock {
  index: number;
  start: number;
  end: number;
  fullMatch: string;
  body: string;
  lang: LiquidFenceLang;
}

export type LiquidCardPointDraft = {
  label: string;
  value: string;
};

export type LiquidCardDraft = {
  title: string;
  subtitle: string;
  emoji: string;
  body: string;
  meta: string;
  points: LiquidCardPointDraft[];
};

export type LiquidCalloutDraft = {
  tone: string;
  title: string;
  body: string;
};

export type LiquidDashboardTileDraft = {
  label: string;
  value: string;
  tone: string;
  delta: string;
};

export type LiquidDashboardDraft = {
  title: string;
  columns: string;
  tiles: LiquidDashboardTileDraft[];
};

export type LiquidTabsPanelDraft = {
  label: string;
  body: string;
};

export type LiquidTabsDraft = {
  title: string;
  defaultLabel: string;
  panels: LiquidTabsPanelDraft[];
};

export type LiquidStepDraft = {
  label: string;
  body: string;
  status: string;
};

export type LiquidStepsDraft = {
  title: string;
  steps: LiquidStepDraft[];
};

export type LiquidAccordionItemDraft = {
  label: string;
  body: string;
  open: boolean;
};

export type LiquidAccordionDraft = {
  title: string;
  multiple: boolean;
  items: LiquidAccordionItemDraft[];
};

export type LiquidCodeDraft = {
  lang: string;
  title: string;
  source: string;
};

export type LiquidTreeDraft = {
  title: string;
  treeText: string;
};

export type LiquidCompareMode = "matrix" | "faceoff";

export type LiquidCompareWidth = "narrow" | "medium" | "wide" | "full";

export type LiquidCompareDraft = {
  title: string;
  subtitle: string;
  recommendation: string;
  mode: LiquidCompareMode;
  /** GFM pipe table body (corner | entities… + axis rows). */
  tableMarkdown: string;
  /** Optional Live / Preview paper width for the compare surface. */
  width?: LiquidCompareWidth;
};

export type LiquidFenceDraft =
  | { lang: "card"; draft: LiquidCardDraft }
  | { lang: "callout"; draft: LiquidCalloutDraft }
  | { lang: "dashboard"; draft: LiquidDashboardDraft }
  | { lang: "tabs"; draft: LiquidTabsDraft }
  | { lang: "steps"; draft: LiquidStepsDraft }
  | { lang: "accordion"; draft: LiquidAccordionDraft }
  | { lang: "code"; draft: LiquidCodeDraft }
  | { lang: "tree"; draft: LiquidTreeDraft }
  | { lang: "compare"; draft: LiquidCompareDraft };

const DEFAULT_COMPARE_TABLE = [
  "| | Option A | Option B |",
  "| --- | --- | --- |",
  "| Axis | … | … |",
].join("\n");

const DASHBOARD_TONES = new Set(["default", "accent", "success", "warn", "error"]);
const DASHBOARD_COLUMNS = new Set(["2", "3", "4"]);
const CALLOUT_TONES = new Set(["note", "warn", "error", "success", "tip", "important"]);
const STEP_STATUSES = new Set(["done", "current", "pending"]);

function fenceRe(lang: LiquidFenceLang): RegExp {
  return new RegExp("```" + lang + "\\s*\\n([\\s\\S]*?)```", "gi");
}

export function extractLiquidFences(
  source: string,
  lang: LiquidFenceLang,
): LiquidFenceBlock[] {
  const blocks: LiquidFenceBlock[] = [];
  const re = fenceRe(lang);
  let match: RegExpExecArray | null;
  let index = 0;
  while ((match = re.exec(source)) !== null) {
    blocks.push({
      index,
      start: match.index,
      end: match.index + match[0].length,
      fullMatch: match[0],
      body: match[1] ?? "",
      lang,
    });
    index += 1;
  }
  return blocks;
}

export function replaceLiquidFenceRawAt(
  source: string,
  lang: LiquidFenceLang,
  index: number,
  raw: string,
): string | null {
  const blocks = extractLiquidFences(source, lang);
  const block = blocks[index];
  if (!block) return null;
  const replacement = raw.trimEnd();
  return source.slice(0, block.start) + replacement + source.slice(block.end);
}

function parseKvLines(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of text.replace(/\r\n/g, "\n").split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#") || line === "---") continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (key && value && !(key in out)) out[key] = value;
  }
  return out;
}

function splitSections(body: string): { preamble: string; sections: string[] } {
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return { preamble: "", sections: [] };
  const parts = normalized.split(/^---[ \t]*$/m);
  return {
    preamble: parts[0] ?? "",
    sections: parts.slice(1).map((s) => s.trim()).filter(Boolean),
  };
}

function splitHeaderBody(body: string): { header: string; rest: string } {
  const normalized = body.replace(/\r\n/g, "\n");
  const sep = normalized.search(/^---[ \t]*$/m);
  if (sep >= 0) {
    return {
      header: normalized.slice(0, sep),
      rest: normalized.slice(sep).replace(/^---[ \t]*\n?/, ""),
    };
  }
  return { header: "", rest: normalized };
}

export function parseCardFenceBody(body: string): LiquidCardDraft {
  const fields = parseKvLines(body);
  const points: LiquidCardPointDraft[] = [];
  for (const raw of body.replace(/\r\n/g, "\n").split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    if (key !== "point" && key !== "points") continue;
    const value = line.slice(colon + 1).trim();
    const pipe = value.indexOf("|");
    if (pipe < 0) {
      if (value) points.push({ label: value, value: "" });
      continue;
    }
    points.push({
      label: value.slice(0, pipe).trim(),
      value: value.slice(pipe + 1).trim(),
    });
  }
  return {
    title: fields.title ?? "",
    subtitle: fields.subtitle ?? "",
    emoji: fields.emoji ?? "",
    body: fields.body ?? "",
    meta: fields.meta ?? "",
    points,
  };
}

export function serializeCardFence(draft: LiquidCardDraft): string {
  const lines = ["```card"];
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  if (draft.subtitle.trim()) lines.push(`subtitle: ${draft.subtitle.trim()}`);
  if (draft.emoji.trim()) lines.push(`emoji: ${draft.emoji.trim()}`);
  if (draft.body.trim()) lines.push(`body: ${draft.body.trim()}`);
  if (draft.meta.trim()) lines.push(`meta: ${draft.meta.trim()}`);
  for (const point of draft.points) {
    const label = point.label.trim();
    const value = point.value.trim();
    if (!label && !value) continue;
    lines.push(value ? `point: ${label} | ${value}` : `point: ${label}`);
  }
  lines.push("```");
  return lines.join("\n") + "\n";
}

export function parseCalloutFenceBody(body: string): LiquidCalloutDraft {
  const fields = parseKvLines(body);
  const toneRaw = (fields.tone ?? "note").toLowerCase();
  return {
    tone: CALLOUT_TONES.has(toneRaw) ? toneRaw : "note",
    title: fields.title ?? "",
    body: fields.body ?? "",
  };
}

export function serializeCalloutFence(draft: LiquidCalloutDraft): string {
  const lines = ["```callout", `tone: ${draft.tone.trim() || "note"}`];
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  lines.push(`body: ${draft.body.trim() || " "}`);
  lines.push("```");
  return lines.join("\n") + "\n";
}

export function parseDashboardFenceBody(body: string): LiquidDashboardDraft {
  const { preamble, sections } = splitSections(body);
  const fields = parseKvLines(preamble);
  const tiles: LiquidDashboardTileDraft[] = [];
  for (const section of sections) {
    const tileFields = parseKvLines(section);
    const label = (tileFields.label ?? tileFields.title ?? "").trim();
    const value = (tileFields.value ?? "").trim();
    if (!label && !value) continue;
    const toneRaw = (tileFields.tone ?? "default").trim().toLowerCase();
    tiles.push({
      label,
      value,
      tone: DASHBOARD_TONES.has(toneRaw) ? toneRaw : "default",
      delta: tileFields.delta ?? "",
    });
  }
  const columnsRaw = (fields.columns ?? "2").trim();
  return {
    title: fields.title ?? "",
    columns: DASHBOARD_COLUMNS.has(columnsRaw) ? columnsRaw : "2",
    tiles:
      tiles.length >= 2
        ? tiles
        : [
            { label: "Metric", value: "—", tone: "default", delta: "" },
            { label: "Status", value: "—", tone: "accent", delta: "" },
          ],
  };
}

export function serializeDashboardFence(draft: LiquidDashboardDraft): string {
  const lines = ["```dashboard"];
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  lines.push(`columns: ${draft.columns.trim() || "2"}`);
  for (const tile of draft.tiles) {
    const label = tile.label.trim();
    const value = tile.value.trim();
    if (!label && !value) continue;
    lines.push("");
    lines.push("---");
    lines.push(`label: ${label || "Metric"}`);
    lines.push(`value: ${value || "—"}`);
    if (tile.tone.trim() && tile.tone !== "default") {
      lines.push(`tone: ${tile.tone.trim()}`);
    }
    if (tile.delta.trim()) lines.push(`delta: ${tile.delta.trim()}`);
  }
  lines.push("```");
  return lines.join("\n") + "\n";
}

export function parseTabsFenceBody(body: string): LiquidTabsDraft {
  const { preamble, sections } = splitSections(body);
  const fields = parseKvLines(preamble);
  const panels: LiquidTabsPanelDraft[] = [];
  for (const section of sections) {
    const item = parseKvLines(section);
    const label = (item.label ?? item.title ?? "").trim();
    const panelBody = (item.body ?? item.summary ?? "").trim();
    if (!label && !panelBody) continue;
    panels.push({ label: label || "Tab", body: panelBody });
  }
  return {
    title: fields.title ?? "",
    defaultLabel: fields.default ?? fields.active ?? "",
    panels:
      panels.length >= 2
        ? panels
        : [
            { label: "Install", body: "…" },
            { label: "Run", body: "…" },
          ],
  };
}

export function serializeTabsFence(draft: LiquidTabsDraft): string {
  const lines = ["```tabs"];
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  if (draft.defaultLabel.trim()) {
    lines.push(`default: ${draft.defaultLabel.trim()}`);
  }
  for (const panel of draft.panels) {
    const label = panel.label.trim();
    const body = panel.body.trim();
    if (!label && !body) continue;
    lines.push("");
    lines.push("---");
    lines.push(`label: ${label || "Tab"}`);
    lines.push(`body: ${body || " "}`);
  }
  lines.push("```");
  return lines.join("\n") + "\n";
}

export function parseStepsFenceBody(body: string): LiquidStepsDraft {
  const { preamble, sections } = splitSections(body);
  const fields = parseKvLines(preamble);
  const steps: LiquidStepDraft[] = [];
  for (const section of sections) {
    const item = parseKvLines(section);
    const label = (item.label ?? item.title ?? "").trim();
    if (!label) continue;
    const statusRaw = (item.status ?? "pending").trim().toLowerCase();
    steps.push({
      label,
      body: (item.body ?? item.summary ?? "").trim(),
      status: STEP_STATUSES.has(statusRaw) ? statusRaw : "pending",
    });
  }
  return {
    title: fields.title ?? "",
    steps:
      steps.length >= 2
        ? steps
        : [
            { label: "Build", body: "…", status: "done" },
            { label: "Ship", body: "…", status: "current" },
          ],
  };
}

export function serializeStepsFence(draft: LiquidStepsDraft): string {
  const lines = ["```steps"];
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  for (const step of draft.steps) {
    const label = step.label.trim();
    if (!label) continue;
    lines.push("");
    lines.push("---");
    lines.push(`label: ${label}`);
    if (step.body.trim()) lines.push(`body: ${step.body.trim()}`);
    if (step.status.trim() && step.status !== "pending") {
      lines.push(`status: ${step.status.trim()}`);
    }
  }
  lines.push("```");
  return lines.join("\n") + "\n";
}

function parseBoolLoose(raw: string | undefined): boolean | undefined {
  if (!raw) return undefined;
  const v = raw.trim().toLowerCase();
  if (["true", "yes", "1", "on", "open"].includes(v)) return true;
  if (["false", "no", "0", "off", "closed"].includes(v)) return false;
  return undefined;
}

export function parseAccordionFenceBody(body: string): LiquidAccordionDraft {
  const { preamble, sections } = splitSections(body);
  const fields = parseKvLines(preamble);
  const items: LiquidAccordionItemDraft[] = [];
  for (const section of sections) {
    const item = parseKvLines(section);
    const label = (item.label ?? item.title ?? "").trim();
    const itemBody = (item.body ?? item.summary ?? "").trim();
    if (!label && !itemBody) continue;
    const open =
      parseBoolLoose(item.open) ??
      parseBoolLoose(item.default) ??
      parseBoolLoose(item.expanded) ??
      false;
    items.push({
      label: label || "Item",
      body: itemBody,
      open,
    });
  }
  return {
    title: fields.title ?? "",
    multiple: parseBoolLoose(fields.multiple) ?? false,
    items:
      items.length >= 1
        ? items
        : [{ label: "What is Liquid?", body: "…", open: true }],
  };
}

export function serializeAccordionFence(draft: LiquidAccordionDraft): string {
  const lines = ["```accordion"];
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  if (draft.multiple) lines.push("multiple: true");
  for (const item of draft.items) {
    const label = item.label.trim();
    const body = item.body.trim();
    if (!label && !body) continue;
    lines.push("");
    lines.push("---");
    lines.push(`label: ${label || "Item"}`);
    lines.push(`body: ${body || " "}`);
    if (item.open) lines.push("open: true");
  }
  lines.push("```");
  return lines.join("\n") + "\n";
}

export function parseCodeFenceBody(body: string): LiquidCodeDraft {
  const { header, rest } = splitHeaderBody(body);
  let fields = parseKvLines(header);
  let source = rest;
  if (!header.trim()) {
    // lang:/title: preamble without ---
    const lines = body.replace(/\r\n/g, "\n").split("\n");
    const preamble: string[] = [];
    let bodyStart = 0;
    for (let i = 0; i < lines.length; i++) {
      const stripped = (lines[i] ?? "").trim();
      if (!stripped) {
        if (preamble.length > 0) {
          bodyStart = i + 1;
          break;
        }
        continue;
      }
      if (/^(lang|language|title|diff|copy)\s*:/i.test(stripped)) {
        preamble.push(lines[i] ?? "");
        bodyStart = i + 1;
        continue;
      }
      bodyStart = i;
      break;
    }
    fields = parseKvLines(preamble.join("\n"));
    source = lines.slice(bodyStart).join("\n");
  }
  return {
    lang: fields.lang ?? fields.language ?? "typescript",
    title: fields.title ?? "",
    source: source.replace(/\s+$/, "").replace(/^\n+/, "") || "// …",
  };
}

export function serializeCodeFence(draft: LiquidCodeDraft): string {
  const lines = ["```code"];
  if (draft.lang.trim()) lines.push(`lang: ${draft.lang.trim()}`);
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  lines.push("---");
  lines.push(draft.source.replace(/\s+$/, "") || "// …");
  lines.push("```");
  return lines.join("\n") + "\n";
}

export function parseTreeFenceBody(body: string): LiquidTreeDraft {
  const { header, rest } = splitHeaderBody(body);
  let fields = parseKvLines(header);
  let treeText = rest;
  if (!header.trim()) {
    const lines = body.replace(/\r\n/g, "\n").split("\n");
    const preamble: string[] = [];
    let bodyStart = 0;
    for (let i = 0; i < lines.length; i++) {
      const stripped = (lines[i] ?? "").trim();
      if (!stripped) {
        if (preamble.length > 0) {
          bodyStart = i + 1;
          break;
        }
        continue;
      }
      if (/^(title|subtitle)\s*:/i.test(stripped)) {
        preamble.push(lines[i] ?? "");
        bodyStart = i + 1;
        continue;
      }
      bodyStart = i;
      break;
    }
    fields = parseKvLines(preamble.join("\n"));
    treeText = lines.slice(bodyStart).join("\n");
  }
  return {
    title: fields.title ?? "",
    treeText: treeText.replace(/\s+$/, "") || "src/\n  index.ts",
  };
}

export function serializeTreeFence(draft: LiquidTreeDraft): string {
  const lines = ["```tree"];
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  lines.push("---");
  lines.push(draft.treeText.replace(/\s+$/, "") || "src/");
  lines.push("```");
  return lines.join("\n") + "\n";
}

function normalizeCompareMode(raw: string | undefined): LiquidCompareMode {
  const modeRaw = (raw ?? "").trim().toLowerCase().replace(/[_ ]+/g, "-");
  if (modeRaw === "faceoff" || modeRaw === "face-off") return "faceoff";
  return "matrix";
}

function normalizeCompareTable(markdown: string): string {
  const table = findFirstPipeTable(markdown.trim()) ?? findFirstPipeTable(DEFAULT_COMPARE_TABLE);
  if (!table) return DEFAULT_COMPARE_TABLE;
  let headers = [...table.headers];
  // Ensure corner + ≥2 entity columns.
  while (headers.length < 3) headers.push(`Option ${headers.length}`);
  if (!headers[0]?.trim()) headers[0] = "";
  let rows =
    table.rows.length > 0
      ? table.rows.map((row) => headers.map((_, i) => row[i] ?? ""))
      : [headers.map((_, i) => (i === 0 ? "Axis" : "…"))];
  if (rows.length < 1) {
    rows = [headers.map((_, i) => (i === 0 ? "Axis" : "…"))];
  }
  // Drop empty trailing entity columns only if we'd still have ≥2 entities.
  while (headers.length > 3) {
    const last = headers.length - 1;
    const entityEmpty = !headers[last]?.trim();
    const cellsEmpty = rows.every((r) => !(r[last] ?? "").trim());
    if (!entityEmpty || !cellsEmpty) break;
    headers = headers.slice(0, last);
    rows = rows.map((r) => r.slice(0, last));
  }
  return serializePipeTable(headers, rows);
}

export function parseCompareFenceBody(body: string): LiquidCompareDraft {
  const normalized = body.replace(/\r\n/g, "\n");
  const table = findFirstPipeTable(normalized);
  let preamble = normalized;
  let tableMarkdown = DEFAULT_COMPARE_TABLE;
  if (table) {
    const lines = normalized.split("\n");
    preamble = lines.slice(0, table.startLine).join("\n");
    tableMarkdown = serializePipeTable(table.headers, table.rows);
  }
  const fields = parseKvLines(preamble);
  const widthRaw = (fields.width ?? "").trim().toLowerCase();
  const width =
    widthRaw === "narrow" ||
    widthRaw === "medium" ||
    widthRaw === "wide" ||
    widthRaw === "full"
      ? widthRaw
      : undefined;
  return {
    title: fields.title ?? "",
    subtitle: fields.subtitle ?? "",
    recommendation: (fields.recommendation ?? fields.highlight ?? "").trim(),
    mode: normalizeCompareMode(fields.mode),
    tableMarkdown: normalizeCompareTable(tableMarkdown),
    ...(width ? { width } : {}),
  };
}

export function serializeCompareFence(draft: LiquidCompareDraft): string {
  const lines = ["```compare"];
  if (draft.title.trim()) lines.push(`title: ${draft.title.trim()}`);
  if (draft.subtitle.trim()) lines.push(`subtitle: ${draft.subtitle.trim()}`);
  if (draft.recommendation.trim()) {
    lines.push(`recommendation: ${draft.recommendation.trim()}`);
  }
  if (draft.mode === "faceoff") lines.push("mode: faceoff");
  if (draft.width && draft.width !== "wide") {
    lines.push(`width: ${draft.width}`);
  }
  lines.push("");
  lines.push(normalizeCompareTable(draft.tableMarkdown));
  lines.push("```");
  return lines.join("\n") + "\n";
}

export function compareEntityLabels(draft: LiquidCompareDraft): string[] {
  const table = findFirstPipeTable(normalizeCompareTable(draft.tableMarkdown));
  if (!table) return ["Option A", "Option B"];
  return table.headers.slice(1).map((h, i) => h.trim() || `Option ${i + 1}`);
}

export function parseLiquidFenceDraft(
  lang: LiquidFenceLang,
  body: string,
): LiquidFenceDraft {
  switch (lang) {
    case "card":
      return { lang, draft: parseCardFenceBody(body) };
    case "callout":
      return { lang, draft: parseCalloutFenceBody(body) };
    case "dashboard":
      return { lang, draft: parseDashboardFenceBody(body) };
    case "tabs":
      return { lang, draft: parseTabsFenceBody(body) };
    case "steps":
      return { lang, draft: parseStepsFenceBody(body) };
    case "accordion":
      return { lang, draft: parseAccordionFenceBody(body) };
    case "code":
      return { lang, draft: parseCodeFenceBody(body) };
    case "tree":
      return { lang, draft: parseTreeFenceBody(body) };
    case "compare":
      return { lang, draft: parseCompareFenceBody(body) };
  }
}

export function serializeLiquidFenceDraft(state: LiquidFenceDraft): string {
  switch (state.lang) {
    case "card":
      return serializeCardFence(state.draft);
    case "callout":
      return serializeCalloutFence(state.draft);
    case "dashboard":
      return serializeDashboardFence(state.draft);
    case "tabs":
      return serializeTabsFence(state.draft);
    case "steps":
      return serializeStepsFence(state.draft);
    case "accordion":
      return serializeAccordionFence(state.draft);
    case "code":
      return serializeCodeFence(state.draft);
    case "tree":
      return serializeTreeFence(state.draft);
    case "compare":
      return serializeCompareFence(state.draft);
  }
}

export function summarizeDashboardTiles(draft: LiquidDashboardDraft): string {
  const n = draft.tiles.filter((t) => t.label.trim() || t.value.trim()).length;
  return n === 1 ? "1 tile" : `${n} tiles`;
}

export function summarizeTabsPanels(draft: LiquidTabsDraft): string {
  const n = draft.panels.filter((p) => p.label.trim() || p.body.trim()).length;
  return n === 1 ? "1 panel" : `${n} panels`;
}

export function summarizeSteps(draft: LiquidStepsDraft): string {
  const n = draft.steps.filter((s) => s.label.trim()).length;
  return n === 1 ? "1 step" : `${n} steps`;
}

export function summarizeAccordionItems(draft: LiquidAccordionDraft): string {
  const n = draft.items.filter((i) => i.label.trim() || i.body.trim()).length;
  return n === 1 ? "1 item" : `${n} items`;
}

export function summarizeCodeSource(draft: LiquidCodeDraft): string {
  const lines = draft.source.split("\n").filter((l) => l.trim()).length;
  return lines === 1 ? "1 line" : `${lines} lines`;
}

export function summarizeTreeText(draft: LiquidTreeDraft): string {
  const lines = draft.treeText.split("\n").filter((l) => l.trim()).length;
  return lines === 1 ? "1 node" : `${lines} lines`;
}

export function summarizeCompareTable(draft: LiquidCompareDraft): string {
  const table = findFirstPipeTable(normalizeCompareTable(draft.tableMarkdown));
  const entities = Math.max(0, (table?.headers.length ?? 1) - 1);
  const axes = table?.rows.length ?? 0;
  if (entities === 1 && axes === 1) return "1×1";
  return `${entities}×${axes}`;
}

export const LIQUID_FENCE_LANGS: LiquidFenceLang[] = [
  "card",
  "callout",
  "dashboard",
  "tabs",
  "steps",
  "accordion",
  "code",
  "tree",
  "compare",
];

export function isLiquidConfigureLang(lang: string): lang is LiquidFenceLang {
  return (LIQUID_FENCE_LANGS as string[]).includes(lang);
}
