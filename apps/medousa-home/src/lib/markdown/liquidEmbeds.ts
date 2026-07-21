/**
 * Liquid markdown embeds — preprocess fences + icon shortcodes into inert
 * placeholders the renderer sanitizes and hydrateLiquidEmbeds mounts.
 *
 * Same pattern as mermaid / wikilinks: model writes familiar markdown; runtime
 * owns the component vocabulary.
 */

import { parseKanbanColumnsFromBody } from "$lib/utils/markdownKanban";
import { parseSlidesDeck } from "$lib/utils/markdownSlides";
import { escapeAttr, escapeHtml } from "./escape";

export const LIQUID_FENCE_LANGS = new Set([
  "card",
  "carousel",
  "actions",
  "action_row",
  "callout",
  "section",
  "chips",
  "chip_group",
  "media",
  "cite",
  "compare",
  "plan",
  "timeline",
  "shortlist",
  "decision",
  "brief",
  "dashboard",
  "chart",
  "report",
  "slides",
  "tabs",
  "steps",
  "accordion",
  "code",
  "tree",
  "kanban",
]);

const CALLOUT_TONES = new Set(["note", "warn", "error", "success"]);
const CHIP_TONES = new Set(["default", "accent", "success", "warn"]);

/** Lucide icon ids allowed in `{{icon:name}}` (kebab or camel). */
export const LIQUID_ICON_ALLOWLIST = new Set([
  "sparkles",
  "lock",
  "globe",
  "message-circle",
  "messagecircle",
  "brain",
  "shield",
  "code",
  "cpu",
  "zap",
  "clock",
  "hourglass",
  "coins",
  "tag",
  "mic",
  "pencil",
  "file-code",
  "filecode",
  "table",
  "layers",
  "rocket",
  "star",
  "check",
  "x",
  "info",
  "alert-triangle",
  "alerttriangle",
  "search",
  "book",
  "map",
  "compass",
]);

export type LiquidEmbedKind =
  | "card"
  | "carousel"
  | "actions"
  | "callout"
  | "section"
  | "chips"
  | "media"
  | "cite"
  | "compare"
  | "plan"
  | "timeline"
  | "shortlist"
  | "decision"
  | "brief"
  | "dashboard"
  | "chart"
  | "report"
  | "slides"
  | "tabs"
  | "steps"
  | "accordion"
  | "code"
  | "tree";

export interface LiquidCardPoint {
  label: string;
  body: string;
  emoji?: string;
}

export interface LiquidCardProps {
  title: string;
  subtitle?: string;
  body?: string;
  emoji?: string;
  image?: string;
  meta?: string;
  summary?: string;
  chips?: string[];
  points?: LiquidCardPoint[];
  badges?: string[];
}

/** Payload for chat card-detail sheet (structured expand). */
export interface CardDetailPayload {
  id: string;
  title: string;
  subtitle?: string;
  emoji?: string;
  image?: string;
  meta?: string;
  summary?: string;
  chips?: string[];
  points?: LiquidCardPoint[];
  badges?: string[];
}

export function cardHasDetail(
  card: Pick<LiquidCardProps, "meta" | "summary" | "chips" | "points">,
): boolean {
  return Boolean(
    (typeof card.meta === "string" && card.meta.trim()) ||
      (typeof card.summary === "string" && card.summary.trim()) ||
      (Array.isArray(card.chips) && card.chips.length > 0) ||
      (Array.isArray(card.points) && card.points.length > 0),
  );
}

export interface LiquidActionProps {
  label: string;
  intent?: string;
  emoji?: string;
}

export interface LiquidCalloutProps {
  body: string;
  tone?: string;
  title?: string;
}

export interface LiquidCiteProps {
  /** At least one of quote / title / url required (enforced by parser). */
  quote?: string;
  title?: string;
  url?: string;
  source?: string;
}

export interface LiquidCompareAxis {
  id: string;
  label: string;
}

export interface LiquidCompareEntity {
  id: string;
  label: string;
  values: Record<string, string>;
}

export type LiquidCompareMode = "matrix" | "faceoff";

export interface LiquidCompareProps {
  title?: string;
  subtitle?: string;
  recommendation?: string;
  /** Presentation: matrix (default) or 2-up faceoff. */
  mode?: LiquidCompareMode;
  axes: LiquidCompareAxis[];
  entities: LiquidCompareEntity[];
}

export interface LiquidPlanSegment {
  id: string;
  label: string;
  time?: string;
  emoji?: string;
  image?: string;
  subtitle?: string;
  body?: string;
  badge?: string;
}

export interface LiquidPlanProps {
  title?: string;
  subtitle?: string;
  grouping?: string;
  segments: LiquidPlanSegment[];
}

export interface LiquidTimelineEvent {
  id: string;
  label: string;
  ts?: string;
  detail?: string;
  lane?: string;
  emoji?: string;
}

export interface LiquidTimelineProps {
  title?: string;
  subtitle?: string;
  granularity?: string;
  events: LiquidTimelineEvent[];
}

export interface LiquidShortlistItem {
  id: string;
  label: string;
  summary?: string;
  score?: string;
  meta?: string;
  emoji?: string;
  image?: string;
}

export interface LiquidShortlistProps {
  title?: string;
  subtitle?: string;
  criteria?: string;
  density?: string;
  items: LiquidShortlistItem[];
}

export interface LiquidDecisionOption {
  id: string;
  label: string;
  pros: string[];
  cons: string[];
  score?: string;
  summary?: string;
}

export interface LiquidDecisionProps {
  title?: string;
  subtitle?: string;
  factors?: string;
  recommendation?: string;
  options: LiquidDecisionOption[];
}

export interface LiquidBriefSection {
  id: string;
  heading: string;
  body: string;
}

export interface LiquidBriefSource {
  id: string;
  title: string;
  url?: string;
  quote?: string;
}

export interface LiquidBriefProps {
  title?: string;
  subtitle?: string;
  tone?: string;
  sections: LiquidBriefSection[];
  sources?: LiquidBriefSource[];
}

export interface LiquidDashboardTile {
  id: string;
  label: string;
  value: string;
  delta?: string;
  tone?: string;
  emoji?: string;
  hint?: string;
  unit?: string;
  feed?: string;
  field?: string;
}

export interface LiquidDashboardProps {
  title?: string;
  subtitle?: string;
  columns?: string;
  tiles: LiquidDashboardTile[];
}

/** Report organism — narrative + nested chart figures in a column grid. */
export interface LiquidReportProps {
  title?: string;
  subtitle?: string;
  /** 1 | 2 | 3 — chart figure columns (prose stays full-bleed). */
  columns?: string;
  /** Markdown body (may include hydrated chart placeholders). */
  body: string;
}

export type LiquidSlideLayout = "hero" | "split" | "stack";
export type LiquidSlideScrim = "dark" | "light" | "none";

export interface LiquidSlideItem {
  id: string;
  label: string;
  layout?: LiquidSlideLayout;
  body: string;
  /** Named wash or image path/URL. */
  bg?: string;
  scrim?: LiquidSlideScrim;
}

/** Slides organism — labeled deck frames with nested figure-grid bodies. */
export interface LiquidSlidesProps {
  title?: string;
  theme?: string;
  columns?: string;
  slides: LiquidSlideItem[];
  /** Preview/export: render every slide (page-break between). */
  showAll?: boolean;
  exportPaper?: boolean;
}

/**
 * Liquid chart organism — paste-first plots from ```chart fences.
 *
 * Renders: bar | line | area | pie | donut | radar | radial | scatter | combo | heatmap.
 * Optional fields (labels, tooltip, legend, interactive, activeKey, curve, layout, …)
 * are accepted so later UI can light up without breaking fences.
 */
export type LiquidChartType =
  | "bar"
  | "line"
  | "area"
  | "pie"
  | "donut"
  | "radar"
  | "radial"
  | "scatter"
  | "combo"
  | "heatmap";

export type LiquidChartLabels = "none" | "value" | "category" | "both";
export type LiquidChartLegend = boolean | "none" | "top" | "bottom";
export type LiquidChartCurve = "smooth" | "linear" | "step";
export type LiquidChartLayout = "vertical" | "horizontal";
export type LiquidChartTrendDirection = "up" | "down" | "flat";
export type LiquidChartLabelPosition = "inside" | "outside" | "auto";
export type LiquidChartSeriesMark = "bar" | "line";

export interface LiquidChartSeries {
  key: string;
  label: string;
  values: number[];
}

/** Scatter point — optional group becomes a legend series. */
export interface LiquidChartPoint {
  x: number;
  y: number;
  group?: string;
}

/** Heatmap matrix — row/col labels + numeric cells. */
export interface LiquidChartMatrix {
  rows: string[];
  cols: string[];
  values: number[][];
}

export interface LiquidChartProps {
  type: LiquidChartType;
  title?: string;
  description?: string;
  categories: string[];
  series: LiquidChartSeries[];

  /** Scatter points (type: scatter). */
  points?: LiquidChartPoint[];
  /** Heatmap matrix (type: heatmap). */
  matrix?: LiquidChartMatrix;
  /** Per-series mark for combo charts (bar | line). */
  seriesMarks?: LiquidChartSeriesMark[];

  layout?: LiquidChartLayout;
  stacked?: boolean;
  curve?: LiquidChartCurve;
  separator?: boolean;
  centerLabel?: string;
  centerValue?: string;

  trend?: string;
  trendDirection?: LiquidChartTrendDirection;
  caption?: string;

  labels?: LiquidChartLabels;
  labelPosition?: LiquidChartLabelPosition;
  tooltip?: boolean;
  legend?: LiquidChartLegend;
  interactive?: boolean;
  activeKey?: string;

  colors?: string[];
  /** Card width: sm | md | lg | full | CSS length (e.g. 18rem, 70%). */
  width?: string;
  /** Plot height for bar/line/area: sm | md | lg | CSS length. */
  height?: string;
  /** Drawing-surface wash (radar plate, polar tracks): soft | muted | none | color. */
  surface?: string;
}

export interface LiquidSectionProps {
  title: string;
  subtitle?: string;
  body?: string;
}

export interface LiquidTabsPanel {
  id: string;
  label: string;
  body: string;
  emoji?: string;
}

export interface LiquidTabsProps {
  title?: string;
  subtitle?: string;
  /** Initial panel id, label, or 1-based index. */
  default?: string;
  panels: LiquidTabsPanel[];
}

export type LiquidStepStatus = "done" | "current" | "pending";

export interface LiquidStepItem {
  id: string;
  label: string;
  body?: string;
  status?: LiquidStepStatus;
  emoji?: string;
}

export interface LiquidStepsProps {
  title?: string;
  subtitle?: string;
  steps: LiquidStepItem[];
}

export interface LiquidAccordionItem {
  id: string;
  label: string;
  body: string;
  open?: boolean;
  emoji?: string;
}

export interface LiquidAccordionProps {
  title?: string;
  subtitle?: string;
  /** Allow multiple panels open at once (default false). */
  multiple?: boolean;
  items: LiquidAccordionItem[];
}

export interface LiquidCodeProps {
  /** Source snippet (required). */
  source: string;
  /** Language badge (js, ts, rust, diff, …). */
  lang?: string;
  title?: string;
  /** When true, render as a unified diff (lines starting with +/-). */
  diff?: boolean;
  /** Show copy button (default true). */
  copy?: boolean;
}

export interface LiquidTreeNode {
  id: string;
  name: string;
  kind: "file" | "folder";
  children?: LiquidTreeNode[];
}

export interface LiquidTreeProps {
  title?: string;
  subtitle?: string;
  nodes: LiquidTreeNode[];
}

export interface LiquidChipProps {
  label: string;
  tone?: string;
  value?: string;
}

export interface LiquidMediaProps {
  src: string;
  alt?: string;
  caption?: string;
  ratio?: string;
}

function encodeProps(value: unknown): string {
  const json = JSON.stringify(value);
  if (typeof btoa === "function") {
    return btoa(unescape(encodeURIComponent(json)));
  }
  // Node / non-DOM fallback without relying on Buffer typings.
  const bytes = new TextEncoder().encode(json);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  if (typeof globalThis.btoa === "function") {
    return globalThis.btoa(binary);
  }
  throw new Error("Base64 encoding is unavailable in this environment");
}

export function decodeLiquidProps<T = unknown>(encoded: string): T | null {
  try {
    let json: string;
    if (typeof atob === "function") {
      json = decodeURIComponent(escape(atob(encoded)));
    } else if (typeof globalThis.atob === "function") {
      json = decodeURIComponent(escape(globalThis.atob(encoded)));
    } else {
      throw new Error("Base64 decoding is unavailable in this environment");
    }
    return JSON.parse(json) as T;
  } catch {
    return null;
  }
}

function placeholder(kind: LiquidEmbedKind, props: unknown, chartIndex?: number): string {
  const embed = `<div class="liquid-md-embed" data-liquid-embed="${escapeAttr(kind)}" data-liquid-props="${escapeAttr(encodeProps(props))}"></div>`;
  if (kind !== "chart" || chartIndex == null) return embed;
  return `<div class="liquid-chart-shell" data-edit-chart-index="${escapeAttr(String(chartIndex))}"><div class="liquid-chart-toolbar"><button type="button" class="liquid-chart-configure">Configure</button></div>${embed}</div>`;
}

/** Document-order chart index while preprocessing (shells + remaining fences). */
function chartEditIndexAt(source: string, matchIndex: number): number {
  const before = source.slice(0, matchIndex);
  const shells = before.match(/class="liquid-chart-shell"/g)?.length ?? 0;
  const fences = before.match(/^```chart\b/gm)?.length ?? 0;
  return shells + fences;
}

/** Models often emit `- title: …` / `* Label: …` inside fences — strip that chrome. */
function stripFenceLineChrome(line: string): string {
  return line
    .trim()
    .replace(/^[-*+]\s+/, "")
    .replace(/^\d+\.\s+/, "");
}

function parseKvLine(line: string): Record<string, string> {
  const out: Record<string, string> = {};
  // Pipe-separated fields: title: Sol | body: Flagship | emoji: 🧠
  const parts = stripFenceLineChrome(line)
    .split("|")
    .map((p) => p.trim())
    .filter(Boolean);
  for (const part of parts) {
    const colon = part.indexOf(":");
    if (colon <= 0) continue;
    const key = part.slice(0, colon).trim().toLowerCase();
    const value = part.slice(colon + 1).trim();
    if (key && value) out[key] = value;
  }
  return out;
}

function parsePipeLabels(raw: string | undefined): string[] {
  if (!raw?.trim()) return [];
  return raw
    .split("|")
    .map((s) => s.trim())
    .filter(Boolean);
}

function parsePointValue(raw: string): LiquidCardPoint | null {
  const parts = raw
    .split("|")
    .map((s) => s.trim())
    .filter(Boolean);
  if (parts.length < 2) return null;
  const [label, body, emoji] = parts;
  if (!label || !body) return null;
  const point: LiquidCardPoint = { label, body };
  if (emoji) point.emoji = emoji;
  return point;
}

/** YAML-ish block scalar markers models emit for multiline card/callout bodies. */
const YAML_BLOCK_SCALAR = /^[|>][+-]?$/;

/**
 * True when a line starts a new `key: value` field (not prose / URLs inside a block).
 * Models often emit unindented `body: |-` blocks; we stop at the next field key.
 */
function isNewKvFieldLine(line: string): boolean {
  const m = line.match(/^([a-zA-Z][a-zA-Z0-9_-]*)\s*:(.*)$/);
  if (!m) return false;
  const key = m[1].toLowerCase();
  // Avoid treating `http://…` / `https://…` as a new field mid-body.
  if (key === "http" || key === "https") return false;
  return true;
}

/**
 * Collect KV fields from a fence body. Supports single-line values and YAML block
 * scalars (`body: |-` / `body: |` / `summary: >-`) so multiline clarification cards
 * don't leak the `|-` marker into the UI.
 */
function parseKvBlock(body: string): Record<string, string> {
  const fields: Record<string, string> = {};
  const lines = body.replace(/\r\n/g, "\n").split("\n");
  let i = 0;

  while (i < lines.length) {
    const line = stripFenceLineChrome(lines[i] ?? "");
    i += 1;
    if (!line || line.startsWith("#") || line === "---") continue;

    // Pipe-separated multi-kv: "title: Sol | body: Flagship"
    if (line.includes("|") && /\|\s*[a-zA-Z][a-zA-Z0-9_-]*\s*:/.test(line)) {
      Object.assign(fields, parseKvLine(line));
      continue;
    }

    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (!key) continue;

    if (YAML_BLOCK_SCALAR.test(value)) {
      const blockLines: string[] = [];
      while (i < lines.length) {
        const next = stripFenceLineChrome(lines[i] ?? "");
        if (next && isNewKvFieldLine(next)) break;
        i += 1;
        if (!next && blockLines.length === 0) continue;
        blockLines.push(next);
      }
      while (blockLines.length && !blockLines[blockLines.length - 1]) {
        blockLines.pop();
      }
      const joined = blockLines.join("\n").trim();
      if (joined) fields[key] = joined;
      continue;
    }

    if (!value) continue;
    fields[key] = value;
  }

  return fields;
}

/** Build a card from a KV/point block (carousel item or single card fence). */
function parseCardBlock(block: string): LiquidCardProps | null {
  const fields: Record<string, string> = {};
  const points: LiquidCardPoint[] = [];
  // Models often dump multi-line post text after a one-line `body:` (or under
  // `title:`) without `|-` / `---`. Those lines used to be dropped.
  const freeform: string[] = [];
  const lines = block.replace(/\r\n/g, "\n").split("\n");
  let i = 0;

  while (i < lines.length) {
    const line = stripFenceLineChrome(lines[i] ?? "");
    i += 1;
    if (!line || line.startsWith("#") || line === "---") {
      // Keep paragraph breaks inside trailing freeform prose.
      if (!line && freeform.length > 0) freeform.push("");
      continue;
    }

    if (line.includes("|") && /\|\s*[a-zA-Z][a-zA-Z0-9_-]*\s*:/.test(line)) {
      const multi = parseKvLine(line);
      for (const [key, value] of Object.entries(multi)) {
        if (key === "point" || key === "points") {
          const point = parsePointValue(value);
          if (point) points.push(point);
          continue;
        }
        if (key === "chips" || key === "badges") {
          fields[key] = fields[key] ? `${fields[key]} | ${value}` : value;
          continue;
        }
        if (!(key in fields)) fields[key] = value;
      }
      continue;
    }

    if (!isNewKvFieldLine(line)) {
      freeform.push(line);
      continue;
    }

    const colon = line.indexOf(":");
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (!key) continue;

    if (YAML_BLOCK_SCALAR.test(value)) {
      const blockLines: string[] = [];
      while (i < lines.length) {
        const next = stripFenceLineChrome(lines[i] ?? "");
        if (next && isNewKvFieldLine(next)) break;
        i += 1;
        if (!next && blockLines.length === 0) continue;
        blockLines.push(next);
      }
      while (blockLines.length && !blockLines[blockLines.length - 1]) {
        blockLines.pop();
      }
      const joined = blockLines.join("\n").trim();
      if (joined && !(key in fields)) fields[key] = joined;
      continue;
    }

    if (!value) continue;

    if (key === "point" || key === "points") {
      const point = parsePointValue(value);
      if (point) points.push(point);
      continue;
    }
    if (key === "chips" || key === "badges") {
      fields[key] = fields[key] ? `${fields[key]} | ${value}` : value;
      continue;
    }
    if (!(key in fields)) fields[key] = value;
  }

  const prose = freeform.join("\n").trim();
  if (prose) {
    if (fields.summary) {
      fields.summary = `${fields.summary.trim()}\n\n${prose}`;
    } else if (fields.body) {
      // Keep short `body:` as the face teaser; sheet gets teaser + continuation.
      fields.summary = `${fields.body.trim()}\n\n${prose}`;
    } else {
      fields.summary = prose;
    }
  }

  const title = fields.title?.trim();
  if (!title) return null;
  const card: LiquidCardProps = { title };
  if (fields.subtitle) card.subtitle = fields.subtitle;
  if (fields.body) card.body = fields.body;
  if (fields.emoji) card.emoji = fields.emoji;
  if (fields.image) card.image = fields.image;
  if (fields.meta) card.meta = fields.meta;
  if (fields.summary) card.summary = fields.summary;
  const chips = parsePipeLabels(fields.chips);
  if (chips.length) card.chips = chips;
  const badges = parsePipeLabels(fields.badges);
  if (badges.length) card.badges = badges;
  if (points.length) card.points = points;
  return card;
}

function parseCardBody(body: string): LiquidCardProps | null {
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const parts = normalized.split(/^---[ \t]*$/m);
  const head = parts[0] ?? "";
  const card = parseCardBlock(head);
  if (!card) return null;

  // Optional --- prose block aliases summary when summary KV absent
  if (!card.summary && parts.length > 1) {
    const prose = parts
      .slice(1)
      .map((p) => p.trim())
      .filter(Boolean)
      .join("\n\n");
    if (prose) card.summary = prose;
  }
  return card;
}

function parseCarouselBody(body: string): LiquidCardProps[] {
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return [];

  // Structured --- item blocks (Monogram expand path)
  if (/^---[ \t]*$/m.test(normalized)) {
    const parts = normalized.split(/^---[ \t]*$/m);
    const cards: LiquidCardProps[] = [];
    for (const block of parts.slice(1)) {
      const card = parseCardBlock(block);
      if (card) cards.push(card);
    }
    return cards;
  }

  // Legacy one-line-per-card
  const cards: LiquidCardProps[] = [];
  for (const raw of normalized.split("\n")) {
    const line = stripFenceLineChrome(raw);
    if (!line || line.startsWith("#")) continue;
    const fields = parseKvLine(line);
    const title = fields.title?.trim();
    if (!title) continue;
    const card: LiquidCardProps = { title };
    if (fields.subtitle) card.subtitle = fields.subtitle;
    if (fields.body) card.body = fields.body;
    if (fields.emoji) card.emoji = fields.emoji;
    if (fields.image) card.image = fields.image;
    const badges = parsePipeLabels(fields.badges);
    if (badges.length) card.badges = badges;
    cards.push(card);
  }
  return cards;
}

function parseActionsBody(body: string): LiquidActionProps[] {
  const actions: LiquidActionProps[] = [];
  for (const raw of body.split("\n")) {
    let line = stripFenceLineChrome(raw);
    if (!line || line.startsWith("#")) continue;
    // Tolerate "Label: Read Raven | intent" / "label: …"
    line = line.replace(/^label\s*:\s*/i, "");
    // "Label | intent" or "emoji Label | intent"
    const pipe = line.indexOf("|");
    let labelPart = pipe >= 0 ? line.slice(0, pipe).trim() : line;
    let intentPart = pipe >= 0 ? line.slice(pipe + 1).trim() : undefined;
    if (intentPart) {
      intentPart = intentPart.replace(/^intent\s*:\s*/i, "").trim() || undefined;
    }
    let emoji: string | undefined;
    const emojiMatch = labelPart.match(
      /^(\p{Extended_Pictographic}|\p{Emoji_Presentation})\s+(.+)$/u,
    );
    if (emojiMatch) {
      emoji = emojiMatch[1];
      labelPart = emojiMatch[2].trim();
    }
    if (!labelPart) continue;
    const action: LiquidActionProps = { label: labelPart };
    if (intentPart) action.intent = intentPart;
    if (emoji) action.emoji = emoji;
    actions.push(action);
  }
  return actions;
}

function parseCalloutBody(body: string): LiquidCalloutProps | null {
  const fields = parseKvBlock(body);
  const text = fields.body?.trim();
  if (!text) return null;
  const callout: LiquidCalloutProps = { body: text };
  const tone = fields.tone?.trim().toLowerCase();
  if (tone && CALLOUT_TONES.has(tone)) callout.tone = tone;
  if (fields.title) callout.title = fields.title;
  return callout;
}

function parseSectionBody(body: string): LiquidSectionProps | null {
  const normalized = body.replace(/\r\n/g, "\n");
  const sep = normalized.search(/^---[ \t]*$/m);
  let header = normalized;
  let proseBody: string | undefined;
  if (sep >= 0) {
    header = normalized.slice(0, sep);
    proseBody = normalized.slice(sep).replace(/^---[ \t]*\n?/, "").trim() || undefined;
  }
  const fields = parseKvBlock(header);
  const title = fields.title?.trim();
  if (!title) return null;
  const section: LiquidSectionProps = { title };
  if (fields.subtitle) section.subtitle = fields.subtitle;
  // Prefer explicit body: field; otherwise use --- prose block
  if (fields.body) section.body = fields.body;
  else if (proseBody) section.body = proseBody;
  return section;
}

function parseChipsBody(body: string): LiquidChipProps[] {
  const chips: LiquidChipProps[] = [];
  for (const raw of body.split("\n")) {
    let line = stripFenceLineChrome(raw);
    if (!line || line.startsWith("#")) continue;
    line = line.replace(/^label\s*:\s*/i, "");
    const pipe = line.indexOf("|");
    const labelPart = pipe >= 0 ? line.slice(0, pipe).trim() : line;
    const rest = pipe >= 0 ? line.slice(pipe + 1).trim() : "";
    if (!labelPart || /^(tone|value)\s*:/i.test(labelPart)) continue;
    const meta: Record<string, string> = {};
    if (rest) {
      for (const part of rest.split("|").map((p) => p.trim()).filter(Boolean)) {
        const colon = part.indexOf(":");
        if (colon <= 0) continue;
        const key = part.slice(0, colon).trim().toLowerCase();
        const value = part.slice(colon + 1).trim();
        if (key && value) meta[key] = value;
      }
    }
    const chip: LiquidChipProps = { label: labelPart };
    const tone = meta.tone?.trim().toLowerCase();
    if (tone && CHIP_TONES.has(tone)) chip.tone = tone;
    if (meta.value) chip.value = meta.value;
    chips.push(chip);
  }
  return chips;
}

function parseMediaBody(body: string): LiquidMediaProps | null {
  const fields = parseKvBlock(body);
  const src = fields.src?.trim();
  if (!src) return null;
  const media: LiquidMediaProps = { src };
  if (fields.alt) media.alt = fields.alt;
  if (fields.caption) media.caption = fields.caption;
  if (fields.ratio) media.ratio = fields.ratio;
  return media;
}

function parseCiteBody(body: string): LiquidCiteProps | null {
  const fields = parseKvBlock(body);
  const cite: LiquidCiteProps = {};
  if (fields.quote) cite.quote = fields.quote;
  if (fields.title) cite.title = fields.title;
  if (fields.url) cite.url = fields.url;
  if (fields.source) cite.source = fields.source;
  // Also accept body: as quote alias
  if (!cite.quote && fields.body) cite.quote = fields.body;
  if (!cite.quote && !cite.title && !cite.url) return null;
  return cite;
}

function slugCompareId(
  label: string,
  prefix: string,
  index: number,
  seen: Map<string, number>,
): string {
  const base = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  const stem = base ? `${prefix}-${base}` : `${prefix}-${index}`;
  const n = seen.get(stem) ?? 0;
  seen.set(stem, n + 1);
  return n === 0 ? stem : `${stem}-${n}`;
}

function splitPipeCells(line: string): string[] {
  let s = stripFenceLineChrome(line);
  if (!s.startsWith("|")) return [];
  if (s.endsWith("|")) s = s.slice(0, -1);
  s = s.slice(1);
  return s.split("|").map((c) => c.trim());
}

function isGfmSeparatorRow(cells: string[]): boolean {
  if (cells.length === 0) return false;
  return cells.every((cell) => {
    const t = cell.replace(/\s/g, "");
    return t === "" || /^:?-+:?$/.test(t);
  });
}

function parseCompareBody(body: string): LiquidCompareProps | null {
  const seenIds = new Map<string, number>();
  const normalized = body.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  const preamble: string[] = [];
  const tableLines: string[] = [];
  let inTable = false;

  for (const raw of lines) {
    const stripped = stripFenceLineChrome(raw);
    if (!inTable && stripped.startsWith("|")) {
      inTable = true;
    }
    if (inTable) {
      if (!stripped) continue;
      tableLines.push(raw);
    } else {
      preamble.push(raw);
    }
  }

  if (tableLines.length < 2) return null;

  const rows = tableLines
    .map((raw) => splitPipeCells(raw))
    .filter((cells) => cells.length > 0)
    .filter((cells) => !isGfmSeparatorRow(cells));

  if (rows.length < 2) return null;

  const header = rows[0];
  if (header.length < 3) return null; // corner + ≥2 entities

  const entityLabels = header.slice(1).map((label) => label.trim()).filter(Boolean);
  if (entityLabels.length < 2) return null;

  const entities: LiquidCompareEntity[] = entityLabels.map((label, i) => ({
    id: slugCompareId(label, "entity", i, seenIds),
    label,
    values: {},
  }));

  const axes: LiquidCompareAxis[] = [];
  for (let r = 1; r < rows.length; r++) {
    const cells = rows[r];
    const axisLabel = (cells[0] ?? "").trim();
    if (!axisLabel) continue;
    const axisId = slugCompareId(axisLabel, "axis", axes.length, seenIds);
    axes.push({ id: axisId, label: axisLabel });
    for (let e = 0; e < entities.length; e++) {
      const value = (cells[e + 1] ?? "").trim();
      if (value) entities[e].values[axisId] = value;
    }
  }

  if (axes.length < 1) return null;

  const fields = parseKvBlock(preamble.join("\n"));
  const compare: LiquidCompareProps = { axes, entities };
  if (fields.title) compare.title = fields.title;
  if (fields.subtitle) compare.subtitle = fields.subtitle;
  const rec = (fields.recommendation ?? fields.highlight)?.trim();
  if (rec) compare.recommendation = rec;
  const modeRaw = (fields.mode ?? "").trim().toLowerCase().replace(/[_ ]+/g, "-");
  if (modeRaw === "faceoff" || modeRaw === "face-off") {
    compare.mode = "faceoff";
  } else if (modeRaw === "matrix") {
    compare.mode = "matrix";
  }
  // Unknown / omitted → render as matrix default (no mode field).
  return compare;
}

const PLAN_GROUPINGS = new Set(["day", "phase", "milestone"]);

function parsePlanBody(body: string): LiquidPlanProps | null {
  const seenIds = new Map<string, number>();
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const parts = normalized.split(/^---[ \t]*$/m);
  const preamble = parts[0] ?? "";
  const segmentBlocks = parts.slice(1);

  // Allow segments without a leading --- if the whole body is one block with label:
  // Prefer ---separated; if no separators, try treating full body as failed.
  const fields = parseKvBlock(preamble);
  const segments: LiquidPlanSegment[] = [];

  for (const block of segmentBlocks) {
    const segFields = parseKvBlock(block);
    const label = (segFields.label ?? segFields.title)?.trim();
    if (!label) continue;
    const seg: LiquidPlanSegment = {
      id: slugCompareId(label, "segment", segments.length, seenIds),
      label,
    };
    if (segFields.time) seg.time = segFields.time;
    if (segFields.emoji) seg.emoji = segFields.emoji;
    if (segFields.image) seg.image = segFields.image;
    if (segFields.subtitle) seg.subtitle = segFields.subtitle;
    if (segFields.body) seg.body = segFields.body;
    if (segFields.badge) seg.badge = segFields.badge;
    segments.push(seg);
  }

  if (segments.length < 2) return null;

  const plan: LiquidPlanProps = { segments };
  if (fields.title) plan.title = fields.title;
  if (fields.subtitle) plan.subtitle = fields.subtitle;
  const grouping = fields.grouping?.trim().toLowerCase();
  if (grouping && PLAN_GROUPINGS.has(grouping)) plan.grouping = grouping;
  return plan;
}

const TIMELINE_GRANULARITIES = new Set(["day", "hour", "event"]);

function parseTimelineBody(body: string): LiquidTimelineProps | null {
  const seenIds = new Map<string, number>();
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const parts = normalized.split(/^---[ \t]*$/m);
  const preamble = parts[0] ?? "";
  const eventBlocks = parts.slice(1);

  const fields = parseKvBlock(preamble);
  const events: LiquidTimelineEvent[] = [];

  for (const block of eventBlocks) {
    const evFields = parseKvBlock(block);
    const label = (evFields.label ?? evFields.title)?.trim();
    if (!label) continue;
    const ev: LiquidTimelineEvent = {
      id: slugCompareId(label, "event", events.length, seenIds),
      label,
    };
    if (evFields.ts) ev.ts = evFields.ts;
    else if (evFields.time) ev.ts = evFields.time;
    if (evFields.detail) ev.detail = evFields.detail;
    else if (evFields.body) ev.detail = evFields.body;
    if (evFields.lane) ev.lane = evFields.lane;
    if (evFields.emoji) ev.emoji = evFields.emoji;
    events.push(ev);
  }

  if (events.length < 2) return null;

  const timeline: LiquidTimelineProps = { events };
  if (fields.title) timeline.title = fields.title;
  if (fields.subtitle) timeline.subtitle = fields.subtitle;
  const granularity = fields.granularity?.trim().toLowerCase();
  if (granularity && TIMELINE_GRANULARITIES.has(granularity)) timeline.granularity = granularity;
  return timeline;
}

const SHORTLIST_DENSITIES = new Set(["comfortable", "compact"]);

function parseShortlistBody(body: string): LiquidShortlistProps | null {
  const seenIds = new Map<string, number>();
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const parts = normalized.split(/^---[ \t]*$/m);
  const preamble = parts[0] ?? "";
  const itemBlocks = parts.slice(1);

  const fields = parseKvBlock(preamble);
  const items: LiquidShortlistItem[] = [];

  for (const block of itemBlocks) {
    const itemFields = parseKvBlock(block);
    const label = (itemFields.label ?? itemFields.title)?.trim();
    if (!label) continue;
    const item: LiquidShortlistItem = {
      id: slugCompareId(label, "item", items.length, seenIds),
      label,
    };
    if (itemFields.summary) item.summary = itemFields.summary;
    else if (itemFields.body) item.summary = itemFields.body;
    if (itemFields.score) item.score = itemFields.score;
    if (itemFields.meta) item.meta = itemFields.meta;
    if (itemFields.emoji) item.emoji = itemFields.emoji;
    if (itemFields.image) item.image = itemFields.image;
    items.push(item);
  }

  if (items.length < 2) return null;

  const shortlist: LiquidShortlistProps = { items };
  if (fields.title) shortlist.title = fields.title;
  if (fields.subtitle) shortlist.subtitle = fields.subtitle;
  if (fields.criteria) shortlist.criteria = fields.criteria;
  const density = fields.density?.trim().toLowerCase();
  if (density && SHORTLIST_DENSITIES.has(density)) shortlist.density = density;
  return shortlist;
}

function splitTradeoffList(raw: string | undefined): string[] {
  if (!raw?.trim()) return [];
  return raw
    .split(/[|\n;]/)
    .map((s) => stripFenceLineChrome(s))
    .map((s) => s.replace(/^(pros?|cons?)\s*:\s*/i, "").trim())
    .filter(Boolean);
}

/** Line kv that keeps pipe characters in values (pros/cons lists). */
function parseDecisionOptionFields(block: string): Record<string, string> {
  const fields: Record<string, string> = {};
  for (const raw of block.split("\n")) {
    const line = stripFenceLineChrome(raw);
    if (!line || line.startsWith("#") || line === "---") continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (!key || !value) continue;
    if (key === "pros" || key === "pro" || key === "cons" || key === "con") {
      fields[key] = fields[key] ? `${fields[key]} | ${value}` : value;
      continue;
    }
    fields[key] = value;
  }
  return fields;
}

function parseDecisionBody(body: string): LiquidDecisionProps | null {
  const seenIds = new Map<string, number>();
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const parts = normalized.split(/^---[ \t]*$/m);
  const preamble = parts[0] ?? "";
  const optionBlocks = parts.slice(1);

  const fields = parseKvBlock(preamble);
  const options: LiquidDecisionOption[] = [];

  for (const block of optionBlocks) {
    const optFields = parseDecisionOptionFields(block);
    const label = (optFields.label ?? optFields.title)?.trim();
    if (!label) continue;
    const pros = splitTradeoffList(optFields.pros ?? optFields.pro);
    const cons = splitTradeoffList(optFields.cons ?? optFields.con);
    const opt: LiquidDecisionOption = {
      id: slugCompareId(label, "option", options.length, seenIds),
      label,
      pros,
      cons,
    };
    if (optFields.score) opt.score = optFields.score;
    if (optFields.summary) opt.summary = optFields.summary;
    else if (optFields.body) opt.summary = optFields.body;
    options.push(opt);
  }

  if (options.length < 2) return null;

  const decision: LiquidDecisionProps = { options };
  if (fields.title) decision.title = fields.title;
  if (fields.subtitle) decision.subtitle = fields.subtitle;
  if (fields.factors) decision.factors = fields.factors;
  const rec = (fields.recommendation ?? fields.highlight)?.trim();
  if (rec) decision.recommendation = rec;
  return decision;
}

const BRIEF_TONES = new Set(["research", "brief", "memo"]);

function stripAtxHeading(line: string): string | null {
  const m = line.trim().match(/^#{1,6}\s+(.+?)(?:\s+#*)?$/);
  if (!m) return null;
  return m[1].replace(/\s+#*$/, "").trim() || null;
}

/** Split a markdown blob on ATX headings into brief sections. */
function parseAtxBriefSections(block: string, startIndex: number): LiquidBriefSection[] {
  const seenIds = new Map<string, number>();
  const lines = block.replace(/\r\n/g, "\n").split("\n");
  const sections: LiquidBriefSection[] = [];
  let heading: string | null = null;
  let bodyLines: string[] = [];
  let leadLines: string[] = [];

  const flush = () => {
    const body = bodyLines.join("\n").trim();
    if (!heading || !body) {
      bodyLines = [];
      return;
    }
    sections.push({
      id: slugCompareId(heading, "section", startIndex + sections.length, seenIds),
      heading,
      body,
    });
    bodyLines = [];
  };

  for (const line of lines) {
    const atx = stripAtxHeading(line);
    if (atx) {
      flush();
      heading = atx;
      continue;
    }
    if (heading) bodyLines.push(line);
    else if (line.trim()) leadLines.push(line);
  }
  flush();

  // Leading prose before the first ## merges into the first ATX section only.
  // Do not invent an "Overview" here — callers handle KV/pending-heading flows.
  const lead = leadLines.join("\n").trim();
  if (lead && sections.length > 0) {
    sections[0] = {
      ...sections[0],
      body: `${lead}\n\n${sections[0].body}`.trim(),
    };
  }

  return sections;
}

function parseBriefSectionBlock(
  block: string,
  index: number,
): LiquidBriefSection | null {
  const seenIds = new Map<string, number>();
  const normalized = block.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  // Prefer explicit heading:/title: + body: (or nested --- prose)
  const sep = normalized.search(/^---[ \t]*$/m);
  let header = normalized;
  let proseBody: string | undefined;
  if (sep >= 0) {
    header = normalized.slice(0, sep);
    proseBody = normalized.slice(sep).replace(/^---[ \t]*\n?/, "").trim() || undefined;
  }
  const fields = parseDecisionOptionFields(header);
  const heading = (fields.heading ?? fields.title)?.trim();
  if (heading) {
    const body = (fields.body ?? proseBody)?.trim();
    if (body) {
      return {
        id: slugCompareId(heading, "section", index, seenIds),
        heading,
        body,
      };
    }
  }

  // Model-common: ## Heading\nprose… (optionally several ## in one --- block)
  const atxSections = parseAtxBriefSections(normalized, index);
  if (atxSections.length === 1) return atxSections[0];
  // Multiple ATX in one block — caller should use parseAtxBriefSections directly
  if (atxSections.length > 1) return atxSections[0];

  return null;
}

function parseBriefSourceBlock(
  block: string,
  index: number,
): LiquidBriefSource | null {
  const seenIds = new Map<string, number>();
  const fields = parseDecisionOptionFields(block);
  const title = (fields.title ?? fields.label)?.trim();
  if (!title) return null;
  const src: LiquidBriefSource = {
    id: slugCompareId(title, "source", index, seenIds),
    title,
  };
  if (fields.url) src.url = fields.url;
  if (fields.quote) src.quote = fields.quote;
  else if (fields.body) src.quote = fields.body;
  return src;
}

function parseBriefBody(body: string): LiquidBriefProps | null {
  const seenIds = new Map<string, number>();
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const eqSplit = normalized.split(/^===[ \t]*$/m);
  const mainPart = (eqSplit[0] ?? "").trim();
  const sourcesPart = eqSplit.length > 1 ? (eqSplit.slice(1).join("\n===\n") ?? "").trim() : "";

  const parts = mainPart.split(/^---[ \t]*$/m);
  const preamble = parts[0] ?? "";
  const sectionBlocks = parts.slice(1);

  const fields = parseKvBlock(preamble);
  const sections: LiquidBriefSection[] = [];
  let pendingHeading: string | null = null;

  for (const block of sectionBlocks) {
    // Multi-heading markdown blob in one --- slice (## A … ## B …)
    if (/^#{1,6}\s+\S/m.test(block)) {
      const atxMany = parseAtxBriefSections(block, sections.length);
      if (atxMany.length >= 1) {
        sections.push(...atxMany);
        pendingHeading = null;
        continue;
      }
    }

    const section = parseBriefSectionBlock(block, sections.length);
    if (section) {
      sections.push(section);
      pendingHeading = null;
      continue;
    }

    const blockFields = parseDecisionOptionFields(block);
    const headingOnly = (blockFields.heading ?? blockFields.title)?.trim();
    if (headingOnly && !blockFields.body) {
      pendingHeading = headingOnly;
      continue;
    }

    if (pendingHeading) {
      const prose = block.replace(/\r\n/g, "\n").trim();
      if (prose) {
        sections.push({
          id: slugCompareId(pendingHeading, "section", sections.length, seenIds),
          heading: pendingHeading,
          body: prose,
        });
        pendingHeading = null;
      }
    }
  }

  // No --- sections: whole body may be ATX markdown (with optional title chrome in preamble)
  if (sections.length < 1) {
    if (sectionBlocks.length === 0 && /^#{1,6}\s+\S/m.test(mainPart)) {
      // Strip KV chrome lines from the top before ATX split when preamble has title:
      const atxSource =
        fields.title || fields.subtitle || fields.tone
          ? mainPart
              .split("\n")
              .filter((line) => {
                const t = stripFenceLineChrome(line);
                if (!t) return true;
                return !/^(title|subtitle|tone)\s*:/i.test(t);
              })
              .join("\n")
          : mainPart;
      sections.push(...parseAtxBriefSections(atxSource, 0));
    } else if (sectionBlocks.length > 0) {
      // Had --- but KV/ATX per-block failed: try ATX on joined remainder, else one Overview
      const rest = sectionBlocks.map((b) => b.trim()).filter(Boolean).join("\n\n");
      if (rest) {
        const atx = /^#{1,6}\s+\S/m.test(rest) ? parseAtxBriefSections(rest, 0) : [];
        if (atx.length) {
          sections.push(...atx);
        } else {
          const heading = fields.title?.trim() || "Overview";
          sections.push({
            id: slugCompareId(heading, "section", 0, seenIds),
            heading,
            body: rest,
          });
        }
      }
    }
  }

  if (sections.length < 1) return null;

  const sources: LiquidBriefSource[] = [];
  if (sourcesPart) {
    const sourceBlocks = sourcesPart.split(/^---[ \t]*$/m).filter((b) => b.trim());
    for (const block of sourceBlocks) {
      const src = parseBriefSourceBlock(block, sources.length);
      if (src) sources.push(src);
    }
  }

  const brief: LiquidBriefProps = { sections };
  if (fields.title) brief.title = fields.title;
  if (fields.subtitle) brief.subtitle = fields.subtitle;
  const tone = fields.tone?.trim().toLowerCase();
  if (tone) {
    // Allowlisted tones preferred; freeform tones (e.g. "warm, analytical") still pass through
    const primary = tone.split(/[,/|]/)[0]?.trim() ?? tone;
    if (BRIEF_TONES.has(primary)) brief.tone = primary;
    else if (BRIEF_TONES.has(tone)) brief.tone = tone;
    else brief.tone = "research";
  }
  if (sources.length) brief.sources = sources;
  return brief;
}

const DASHBOARD_TONES = new Set(["default", "accent", "success", "warn", "error"]);
const DASHBOARD_COLUMNS = new Set(["2", "3", "4"]);

function parseDashboardBody(body: string): LiquidDashboardProps | null {
  const seenIds = new Map<string, number>();
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const parts = normalized.split(/^---[ \t]*$/m);
  const preamble = parts[0] ?? "";
  const tileBlocks = parts.slice(1);

  const fields = parseKvBlock(preamble);
  const tiles: LiquidDashboardTile[] = [];

  for (const block of tileBlocks) {
    const tileFields = parseKvBlock(block);
    const label = (tileFields.label ?? tileFields.title)?.trim();
    const value = tileFields.value?.trim();
    if (!label || !value) continue;
    // binding: ignored (wave C+); feed:/field: hydrate for tail-from-chat
    const tile: LiquidDashboardTile = {
      id: slugCompareId(label, "tile", tiles.length, seenIds),
      label,
      value,
    };
    if (tileFields.delta) tile.delta = tileFields.delta;
    if (tileFields.emoji) tile.emoji = tileFields.emoji;
    if (tileFields.hint) tile.hint = tileFields.hint;
    else if (tileFields.body) tile.hint = tileFields.body;
    if (tileFields.unit) tile.unit = tileFields.unit;
    const tone = tileFields.tone?.trim().toLowerCase();
    if (tone && DASHBOARD_TONES.has(tone)) tile.tone = tone;
    const feed = tileFields.feed?.trim();
    if (feed) tile.feed = feed;
    const field = tileFields.field?.trim();
    if (field) tile.field = field;
    tiles.push(tile);
  }

  if (tiles.length < 2) return null;

  const dashboard: LiquidDashboardProps = { tiles };
  if (fields.title) dashboard.title = fields.title;
  if (fields.subtitle) dashboard.subtitle = fields.subtitle;
  const columns = fields.columns?.trim();
  if (columns && DASHBOARD_COLUMNS.has(columns)) dashboard.columns = columns;
  return dashboard;
}

const REPORT_COLUMNS = new Set(["1", "2", "3"]);

function parseReportBody(body: string): LiquidReportProps | null {
  const normalized = body.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  const preamble: string[] = [];
  let bodyStart = 0;

  for (let i = 0; i < lines.length; i++) {
    const stripped = stripFenceLineChrome(lines[i] ?? "");
    if (!stripped) {
      // blank line after KV ends preamble
      if (preamble.length > 0) {
        bodyStart = i + 1;
        break;
      }
      continue;
    }
    // KV line (title: …) stays in preamble; first non-KV starts body
    if (/^[a-zA-Z][a-zA-Z0-9_-]*\s*:/.test(stripped) && !stripped.startsWith("|")) {
      preamble.push(lines[i] ?? "");
      bodyStart = i + 1;
      continue;
    }
    bodyStart = i;
    break;
  }

  const fields = parseKvBlock(preamble.join("\n"));
  const markdownBody = lines.slice(bodyStart).join("\n").trim();
  if (!markdownBody && !fields.title) return null;

  const report: LiquidReportProps = { body: markdownBody || "" };
  if (fields.title) report.title = fields.title;
  if (fields.subtitle) report.subtitle = fields.subtitle;
  const columns = fields.columns?.trim();
  if (columns && REPORT_COLUMNS.has(columns)) report.columns = columns;
  else report.columns = "2";
  return report;
}

const STEP_STATUSES = new Set(["done", "current", "pending"]);

/** Shared --- panel/item blocks for tabs / steps / accordion. */
function parseLabeledSectionBlocks(
  body: string,
): { fields: Record<string, string>; blocks: Record<string, string>[] } | null {
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const parts = normalized.split(/^---[ \t]*$/m);
  const preamble = parts[0] ?? "";
  const sectionBlocks = parts.slice(1);
  if (sectionBlocks.length < 1) return null;

  const fields = parseKvBlock(preamble);
  const blocks: Record<string, string>[] = [];

  for (const block of sectionBlocks) {
    const sep = block.search(/^---[ \t]*$/m);
    let header = block;
    let proseBody: string | undefined;
    if (sep >= 0) {
      header = block.slice(0, sep);
      proseBody = block.slice(sep).replace(/^---[ \t]*\n?/, "").trim() || undefined;
    }
    const itemFields = parseKvBlock(header);
    if (proseBody && !itemFields.body) itemFields.body = proseBody;
    // Accept freeform prose-only --- blocks as body when a prior label is pending —
    // but require label/title on the block itself for paste-first clarity.
    const label = (itemFields.label ?? itemFields.title)?.trim();
    if (!label) continue;
    if (!itemFields.body?.trim() && !itemFields.summary?.trim()) {
      // Allow label-only steps; tabs/accordion need body — callers enforce.
      blocks.push(itemFields);
      continue;
    }
    if (!itemFields.body && itemFields.summary) itemFields.body = itemFields.summary;
    blocks.push(itemFields);
  }

  if (blocks.length < 1) return null;
  return { fields, blocks };
}

function parseTabsBody(body: string): LiquidTabsProps | null {
  const seenIds = new Map<string, number>();
  const parsed = parseLabeledSectionBlocks(body);
  if (!parsed) return null;

  const panels: LiquidTabsPanel[] = [];
  for (const block of parsed.blocks) {
    const label = (block.label ?? block.title)?.trim();
    const panelBody = (block.body ?? block.summary)?.trim();
    if (!label || !panelBody) continue;
    const panel: LiquidTabsPanel = {
      id: slugCompareId(label, "tab", panels.length, seenIds),
      label,
      body: panelBody,
    };
    if (block.emoji) panel.emoji = block.emoji;
    panels.push(panel);
  }
  if (panels.length < 2) return null;

  const tabs: LiquidTabsProps = { panels };
  if (parsed.fields.title) tabs.title = parsed.fields.title;
  if (parsed.fields.subtitle) tabs.subtitle = parsed.fields.subtitle;
  const def = (parsed.fields.default ?? parsed.fields.active)?.trim();
  if (def) tabs.default = def;
  return tabs;
}

function parseStepsBody(body: string): LiquidStepsProps | null {
  const seenIds = new Map<string, number>();
  const parsed = parseLabeledSectionBlocks(body);
  if (!parsed) return null;

  const steps: LiquidStepItem[] = [];
  for (const block of parsed.blocks) {
    const label = (block.label ?? block.title)?.trim();
    if (!label) continue;
    const step: LiquidStepItem = {
      id: slugCompareId(label, "step", steps.length, seenIds),
      label,
    };
    const stepBody = (block.body ?? block.summary)?.trim();
    if (stepBody) step.body = stepBody;
    if (block.emoji) step.emoji = block.emoji;
    const status = block.status?.trim().toLowerCase();
    if (status && STEP_STATUSES.has(status)) step.status = status as LiquidStepStatus;
    steps.push(step);
  }
  if (steps.length < 2) return null;

  const out: LiquidStepsProps = { steps };
  if (parsed.fields.title) out.title = parsed.fields.title;
  if (parsed.fields.subtitle) out.subtitle = parsed.fields.subtitle;
  return out;
}

function parseBoolLoose(raw: string | undefined): boolean | undefined {
  if (!raw) return undefined;
  const v = raw.trim().toLowerCase();
  if (["true", "yes", "1", "on", "open"].includes(v)) return true;
  if (["false", "no", "0", "off", "closed"].includes(v)) return false;
  return undefined;
}

function parseAccordionBody(body: string): LiquidAccordionProps | null {
  const seenIds = new Map<string, number>();
  const parsed = parseLabeledSectionBlocks(body);
  if (!parsed) return null;

  const items: LiquidAccordionItem[] = [];
  for (const block of parsed.blocks) {
    const label = (block.label ?? block.title)?.trim();
    const itemBody = (block.body ?? block.summary)?.trim();
    if (!label || !itemBody) continue;
    const item: LiquidAccordionItem = {
      id: slugCompareId(label, "item", items.length, seenIds),
      label,
      body: itemBody,
    };
    if (block.emoji) item.emoji = block.emoji;
    const open =
      parseBoolLoose(block.open) ??
      parseBoolLoose(block.default) ??
      parseBoolLoose(block.expanded);
    if (open !== undefined) item.open = open;
    items.push(item);
  }
  if (items.length < 1) return null;

  const accordion: LiquidAccordionProps = { items };
  if (parsed.fields.title) accordion.title = parsed.fields.title;
  if (parsed.fields.subtitle) accordion.subtitle = parsed.fields.subtitle;
  const multiple = parseBoolLoose(parsed.fields.multiple);
  if (multiple !== undefined) accordion.multiple = multiple;
  return accordion;
}

function parseCodeBody(body: string): LiquidCodeProps | null {
  const normalized = body.replace(/\r\n/g, "\n");
  // Prefer --- separator: KV header + source body
  const sep = normalized.search(/^---[ \t]*$/m);
  let header = "";
  let source = "";
  if (sep >= 0) {
    header = normalized.slice(0, sep);
    source = normalized.slice(sep).replace(/^---[ \t]*\n?/, "");
  } else {
    // No --- : require lang: (or language:) KV, then remainder is source
    const lines = normalized.split("\n");
    const preamble: string[] = [];
    let bodyStart = 0;
    for (let i = 0; i < lines.length; i++) {
      const stripped = stripFenceLineChrome(lines[i] ?? "");
      if (!stripped) {
        if (preamble.length > 0) {
          bodyStart = i + 1;
          break;
        }
        continue;
      }
      if (
        /^(lang|language|title|diff|copy)\s*:/i.test(stripped) &&
        !stripped.startsWith("|")
      ) {
        preamble.push(lines[i] ?? "");
        bodyStart = i + 1;
        continue;
      }
      bodyStart = i;
      break;
    }
    header = preamble.join("\n");
    source = lines.slice(bodyStart).join("\n");
  }

  const fields = parseKvBlock(header);
  const trimmedSource = source.replace(/\s+$/, "").replace(/^\n+/, "");
  if (!trimmedSource) return null;

  // Require intentional liquid shape: lang/language/title/diff present, or --- was used
  const hasLiquidChrome =
    sep >= 0 ||
    Boolean(fields.lang || fields.language || fields.title || fields.diff || fields.copy);
  if (!hasLiquidChrome) return null;

  const code: LiquidCodeProps = { source: trimmedSource };
  const lang = (fields.lang ?? fields.language)?.trim();
  if (lang) code.lang = lang;
  if (fields.title) code.title = fields.title;
  const diff = parseBoolLoose(fields.diff);
  if (diff !== undefined) code.diff = diff;
  else if (lang?.toLowerCase() === "diff") code.diff = true;
  const copy = parseBoolLoose(fields.copy);
  if (copy !== undefined) code.copy = copy;
  return code;
}

function parseTreeIndentedLines(raw: string): LiquidTreeNode[] {
  const lines = raw.replace(/\r\n/g, "\n").split("\n");
  type StackEntry = { indent: number; node: LiquidTreeNode };
  const roots: LiquidTreeNode[] = [];
  const stack: StackEntry[] = [];
  let counter = 0;

  for (const rawLine of lines) {
    if (!rawLine.trim() || rawLine.trim().startsWith("#")) continue;
    // Preserve leading spaces; tabs → 2 spaces
    const expanded = rawLine.replace(/\t/g, "  ").replace(/\r$/, "");
    const trimmed = expanded.trimEnd();
    const match = trimmed.match(/^( *)(.*)$/);
    if (!match) continue;
    const indent = match[1].length;
    let name = match[2].trim();
    // Strip list markers models often emit
    // Strip list markers / ascii tree chrome models often emit
    name = name
      .replace(/^[-*+]\s+/, "")
      .replace(/^[|\\+\-`]+[─\-]*\s*/, "")
      .trim();
    if (!name) continue;

    let kind: "file" | "folder" = "file";
    if (name.endsWith("/")) {
      kind = "folder";
      name = name.slice(0, -1).trim();
    } else if (!name.includes(".") && /\/$/.test(match[2].trim())) {
      kind = "folder";
    }
    // Heuristic: trailing slash already handled; bare directory words kept as files unless /
    const node: LiquidTreeNode = {
      id: `tree-${counter++}`,
      name,
      kind,
    };
    if (kind === "folder") node.children = [];

    while (stack.length && stack[stack.length - 1].indent >= indent) {
      stack.pop();
    }

    if (stack.length === 0) {
      roots.push(node);
    } else {
      const parent = stack[stack.length - 1].node;
      if (!parent.children) parent.children = [];
      parent.kind = "folder";
      parent.children.push(node);
    }
    stack.push({ indent, node });
  }

  return roots;
}

function parseTreeBody(body: string): LiquidTreeProps | null {
  const normalized = body.replace(/\r\n/g, "\n");
  const sep = normalized.search(/^---[ \t]*$/m);
  let header = "";
  let treeBody = normalized;
  if (sep >= 0) {
    header = normalized.slice(0, sep);
    treeBody = normalized.slice(sep).replace(/^---[ \t]*\n?/, "");
  } else {
    // Optional leading KV lines (title/subtitle only)
    const lines = normalized.split("\n");
    const preamble: string[] = [];
    let bodyStart = 0;
    for (let i = 0; i < lines.length; i++) {
      const stripped = stripFenceLineChrome(lines[i] ?? "");
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
    header = preamble.join("\n");
    treeBody = lines.slice(bodyStart).join("\n");
  }

  const fields = parseKvBlock(header);
  const nodes = parseTreeIndentedLines(treeBody);
  if (nodes.length < 1) return null;

  const tree: LiquidTreeProps = { nodes };
  if (fields.title) tree.title = fields.title;
  if (fields.subtitle) tree.subtitle = fields.subtitle;
  return tree;
}

const CHART_TYPES = new Set<LiquidChartType>([
  "bar",
  "line",
  "area",
  "pie",
  "donut",
  "radar",
  "radial",
  "scatter",
  "combo",
  "heatmap",
]);
const CHART_LABELS = new Set(["none", "value", "category", "both"]);
const CHART_LEGEND = new Set(["none", "top", "bottom", "true", "false", "yes", "no", "1", "0"]);
const CHART_CURVE = new Set(["smooth", "linear", "step"]);
const CHART_LAYOUT = new Set(["vertical", "horizontal"]);
const CHART_TREND_DIR = new Set(["up", "down", "flat"]);
const CHART_LABEL_POS = new Set(["inside", "outside", "auto"]);
const CHART_SERIES_MARK = new Set(["bar", "line"]);

function parseChartNumber(raw: string): number | null {
  const cleaned = raw.replace(/,/g, "").replace(/%$/, "").trim();
  if (!cleaned) return null;
  const n = Number(cleaned);
  return Number.isFinite(n) ? n : null;
}

function parseChartBool(raw: string | undefined): boolean | undefined {
  if (!raw) return undefined;
  const v = raw.trim().toLowerCase();
  if (["true", "yes", "1", "on"].includes(v)) return true;
  if (["false", "no", "0", "off"].includes(v)) return false;
  return undefined;
}

function parseChartLegend(raw: string | undefined): LiquidChartLegend | undefined {
  if (!raw) return undefined;
  const v = raw.trim().toLowerCase();
  if (v === "none" || v === "top" || v === "bottom") return v;
  const asBool = parseChartBool(v);
  if (asBool !== undefined) return asBool;
  return undefined;
}

function parseChartBody(body: string): LiquidChartProps | null {
  const normalized = body.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  const preamble: string[] = [];
  const tableLines: string[] = [];
  let inTable = false;

  for (const raw of lines) {
    const stripped = stripFenceLineChrome(raw);
    if (!inTable && stripped.startsWith("|")) {
      inTable = true;
    }
    if (inTable) {
      if (!stripped) continue;
      tableLines.push(raw);
    } else {
      preamble.push(raw);
    }
  }

  if (tableLines.length < 2) return null;

  const rows = tableLines
    .map((raw) => splitPipeCells(raw))
    .filter((cells) => cells.length > 0)
    .filter((cells) => !isGfmSeparatorRow(cells));

  if (rows.length < 2) return null;

  const fields = parseKvBlock(preamble.join("\n"));
  const typeRaw = (fields.type ?? fields.chart ?? "bar").trim().toLowerCase();
  if (!CHART_TYPES.has(typeRaw as LiquidChartType)) return null;
  const type = typeRaw as LiquidChartType;

  let chart: LiquidChartProps | null = null;
  if (type === "scatter") {
    chart = parseScatterTable(rows, type);
  } else if (type === "heatmap") {
    chart = parseHeatmapTable(rows, type);
  } else {
    chart = parseCategorySeriesTable(rows, type);
  }
  if (!chart) return null;

  applyChartKvFields(chart, fields);
  return chart;
}

function parseCategorySeriesTable(
  rows: string[][],
  type: LiquidChartType,
): LiquidChartProps | null {
  const seenIds = new Map<string, number>();
  const header = rows[0].map((cell) => cell.trim());
  if (header.length < 2) return null;

  const seriesHeaders = header.slice(1).map((label) => label.trim()).filter(Boolean);
  if (seriesHeaders.length < 1) return null;

  const categories: string[] = [];
  const seriesValues: number[][] = seriesHeaders.map(() => []);

  for (let r = 1; r < rows.length; r++) {
    const cells = rows[r];
    const category = (cells[0] ?? "").trim();
    if (!category) continue;
    const nums: number[] = [];
    let ok = true;
    for (let s = 0; s < seriesHeaders.length; s++) {
      const n = parseChartNumber(cells[s + 1] ?? "");
      if (n === null) {
        ok = false;
        break;
      }
      nums.push(n);
    }
    if (!ok) continue;
    categories.push(category);
    for (let s = 0; s < seriesHeaders.length; s++) {
      seriesValues[s].push(nums[s]);
    }
  }

  const minCats = type === "radar" ? 3 : type === "radial" ? 1 : 2;
  if (categories.length < minCats) return null;

  const series: LiquidChartSeries[] = seriesHeaders.map((label, i) => ({
    key: slugCompareId(label, "series", i, seenIds),
    label,
    values: seriesValues[i],
  }));

  return { type, categories, series };
}

function parseScatterTable(rows: string[][], type: LiquidChartType): LiquidChartProps | null {
  const seenIds = new Map<string, number>();
  const header = rows[0].map((cell) => cell.trim());
  if (header.length < 2) return null;
  const hasGroup = header.length >= 3;

  const points: LiquidChartPoint[] = [];
  for (let r = 1; r < rows.length; r++) {
    const cells = rows[r];
    const x = parseChartNumber(cells[0] ?? "");
    const y = parseChartNumber(cells[1] ?? "");
    if (x === null || y === null) continue;
    const point: LiquidChartPoint = { x, y };
    if (hasGroup) {
      const group = (cells[2] ?? "").trim();
      if (group) point.group = group;
    }
    points.push(point);
  }
  if (points.length < 2) return null;

  const groupNames = [
    ...new Set(points.map((p) => p.group).filter((g): g is string => Boolean(g))),
  ];
  const series: LiquidChartSeries[] =
    groupNames.length > 0
      ? groupNames.map((label, i) => ({
          key: slugCompareId(label, "series", i, seenIds),
          label,
          values: points.filter((p) => p.group === label).map((p) => p.y),
        }))
      : [{ key: "points", label: header[1] || "Y", values: points.map((p) => p.y) }];

  return {
    type,
    categories: groupNames.length ? groupNames : ["Points"],
    series,
    points,
  };
}

function parseHeatmapTable(rows: string[][], type: LiquidChartType): LiquidChartProps | null {
  const seenIds = new Map<string, number>();
  const header = rows[0].map((cell) => cell.trim());
  if (header.length < 2) return null;
  const cols = header.slice(1).map((c) => c.trim()).filter(Boolean);
  if (cols.length < 1) return null;

  const rowLabels: string[] = [];
  const values: number[][] = [];

  for (let r = 1; r < rows.length; r++) {
    const cells = rows[r];
    const rowLabel = (cells[0] ?? "").trim();
    if (!rowLabel) continue;
    const nums: number[] = [];
    let ok = true;
    for (let c = 0; c < cols.length; c++) {
      const n = parseChartNumber(cells[c + 1] ?? "");
      if (n === null) {
        ok = false;
        break;
      }
      nums.push(n);
    }
    if (!ok) continue;
    rowLabels.push(rowLabel);
    values.push(nums);
  }

  if (rowLabels.length < 1 || values.length < 1) return null;

  return {
    type,
    categories: cols,
    series: rowLabels.map((label, i) => ({
      key: slugCompareId(label, "row", i, seenIds),
      label,
      values: values[i],
    })),
    matrix: { rows: rowLabels, cols, values },
  };
}

function applyChartKvFields(
  chart: LiquidChartProps,
  fields: Record<string, string>,
): void {
  if (fields.title) chart.title = fields.title;
  const description = (fields.description ?? fields.subtitle)?.trim();
  if (description) chart.description = description;

  const layout = fields.layout?.trim().toLowerCase();
  if (layout && CHART_LAYOUT.has(layout)) {
    chart.layout = layout as LiquidChartLayout;
  }

  const stacked = parseChartBool(fields.stacked);
  if (stacked !== undefined) chart.stacked = stacked;

  const curve = fields.curve?.trim().toLowerCase();
  if (curve && CHART_CURVE.has(curve)) {
    chart.curve = curve as LiquidChartCurve;
  }

  const separator = parseChartBool(fields.separator);
  if (separator !== undefined) chart.separator = separator;

  if (fields.centerlabel) chart.centerLabel = fields.centerlabel;
  else if (fields["center_label"]) chart.centerLabel = fields["center_label"];
  if (fields.centervalue) chart.centerValue = fields.centervalue;
  else if (fields["center_value"]) chart.centerValue = fields["center_value"];

  if (fields.trend) chart.trend = fields.trend;
  const trendDirection = (fields.trenddirection ?? fields["trend_direction"])
    ?.trim()
    .toLowerCase();
  if (trendDirection && CHART_TREND_DIR.has(trendDirection)) {
    chart.trendDirection = trendDirection as LiquidChartTrendDirection;
  }
  if (fields.caption) chart.caption = fields.caption;

  const labels = fields.labels?.trim().toLowerCase();
  if (labels && CHART_LABELS.has(labels)) {
    chart.labels = labels as LiquidChartLabels;
  }
  const labelPosition = (fields.labelposition ?? fields["label_position"])
    ?.trim()
    .toLowerCase();
  if (labelPosition && CHART_LABEL_POS.has(labelPosition)) {
    chart.labelPosition = labelPosition as LiquidChartLabelPosition;
  }

  const tooltip = parseChartBool(fields.tooltip);
  if (tooltip !== undefined) chart.tooltip = tooltip;

  if (fields.legend && CHART_LEGEND.has(fields.legend.trim().toLowerCase())) {
    chart.legend = parseChartLegend(fields.legend);
  }

  const interactive = parseChartBool(fields.interactive);
  if (interactive !== undefined) chart.interactive = interactive;

  const activeKey = (fields.activekey ?? fields["active_key"])?.trim();
  if (activeKey) chart.activeKey = activeKey;

  const colorsRaw = fields.colors?.trim();
  if (colorsRaw) {
    const colors = colorsRaw
      .split(/[,|]/)
      .map((c) => c.trim())
      .filter(Boolean);
    if (colors.length) chart.colors = colors;
  }

  if (fields.width?.trim()) chart.width = fields.width.trim();
  if (fields.height?.trim()) chart.height = fields.height.trim();
  const surface = (fields.surface ?? fields.plot)?.trim();
  if (surface) chart.surface = surface;

  const marksRaw = (fields.seriesmarks ?? fields["series_marks"])?.trim();
  if (marksRaw && chart.type === "combo") {
    const marks = marksRaw
      .split(/[,|]/)
      .map((m) => m.trim().toLowerCase())
      .filter((m): m is LiquidChartSeriesMark => CHART_SERIES_MARK.has(m));
    if (marks.length) chart.seriesMarks = marks;
  }
}

function normalizeIconId(raw: string): string | null {
  const id = raw.trim().toLowerCase().replace(/_/g, "-");
  if (!id || !LIQUID_ICON_ALLOWLIST.has(id)) return null;
  // Canonical kebab form for data attribute
  return id.replace(/messagecircle/, "message-circle")
    .replace(/filecode/, "file-code")
    .replace(/alerttriangle/, "alert-triangle");
}

const PROSE_MISTAKEN_FENCE_LANGS = new Set([
  "code",
  "text",
  "plaintext",
  "plain",
  "markdown",
  "md",
  "output",
  "response",
  "answer",
  "txt",
]);

/** True when a fenced body looks like narrative prose, not source code. */
export function looksLikeProseNotCode(body: string): boolean {
  const t = body.replace(/\r\n/g, "\n").trim();
  if (t.length < 20) return false;
  if (/```/.test(t)) return false;
  if (
    /^(import |export |from ['"]|const |let |var |function |class |def |package |using |#include|<\?php|#!\/)/m.test(
      t,
    )
  ) {
    return false;
  }
  const lines = t.split("\n");
  const codey = lines.filter((l) =>
    /^\s*[{}\]);]|^\s*\/\/|^\s*\/\*|=>|:=|;\s*$/.test(l),
  ).length;
  if (codey >= 3) return false;
  if (/\*\*[^*]+\*\*|(^|[^*])\*[^*\n]+\*([^*]|$)|_[^_\n]+_/.test(t)) return true;
  // Narrative sentence(s) without code punctuation density
  if (/[.!?]["'”’)]?\s*$/.test(t) && t.length >= 40) return true;
  if ((t.match(/[.!?][\s"'”’]/g) ?? []).length >= 1 && t.length > 48) return true;
  return false;
}

/**
 * Strip a trailing unclosed ``` / ```code opener when the remainder is prose
 * (models often leave a bare fence before the final statement).
 *
 * Uses CommonMark open/close pairing — ```tabs inside a docs sample must not
 * count as a closer, or leftover trailing ``` wrappers get treated as balanced.
 */
function repairTrailingUnclosedProseFence(source: string): string {
  const lines = source.split("\n");
  let depth = 0;
  let openLine = -1;
  let openLang = "";
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (depth === 0) {
      const open = matchFenceOpen(line);
      if (!open) continue;
      depth = 1;
      openLine = i;
      openLang = open.lang;
      continue;
    }
    const open = matchFenceOpen(lines[openLine]!);
    const ticks = open?.ticks ?? 3;
    if (matchFenceClose(line, ticks)) {
      depth = 0;
      openLine = -1;
      openLang = "";
    }
  }
  if (depth === 0 || openLine < 0) return source;

  const after = lines.slice(openLine + 1).join("\n");
  if (/^```/m.test(after)) return source;
  if (openLang && LIQUID_FENCE_LANGS.has(openLang)) return source;
  if (!after.trim()) {
    return lines.slice(0, openLine).join("\n");
  }
  if (
    openLang === "" ||
    PROSE_MISTAKEN_FENCE_LANGS.has(openLang) ||
    looksLikeProseNotCode(after)
  ) {
    return [...lines.slice(0, openLine), ...lines.slice(openLine + 1)].join("\n");
  }
  return source;
}

function replaceLiquidFenceMatch(
  match: string,
  langRaw: string,
  body: string,
  fullSource: string,
  matchIndex: number,
): string {
  const lang = langRaw.trim().toLowerCase();

  // Liquid ```code (lang:/title:/---) wins over mistaken-prose unwrap of ```code
  if (lang === "code") {
    const code = parseCodeBody(body);
    if (code) return `\n${placeholder("code", code)}\n`;
    if (looksLikeProseNotCode(body)) {
      return `\n${body.trim()}\n`;
    }
    return match;
  }

  // Models wrap final statements in bare ``` / ```text — unwrap when clearly prose
  if (
    (lang === "" || PROSE_MISTAKEN_FENCE_LANGS.has(lang)) &&
    looksLikeProseNotCode(body)
  ) {
    return `\n${body.trim()}\n`;
  }

  if (!lang || !LIQUID_FENCE_LANGS.has(lang)) return match;

  if (lang === "card") {
    const card = parseCardBody(body);
    if (!card) return match;
    return `\n${placeholder("card", card)}\n`;
  }

  if (lang === "carousel") {
    const cards = parseCarouselBody(body);
    if (cards.length === 0) return match;
    return `\n${placeholder("carousel", { items: cards })}\n`;
  }

  if (lang === "actions" || lang === "action_row") {
    const actions = parseActionsBody(body);
    if (actions.length === 0) return match;
    return `\n${placeholder("actions", { actions })}\n`;
  }

  if (lang === "callout") {
    const callout = parseCalloutBody(body);
    if (!callout) return match;
    return `\n${placeholder("callout", callout)}\n`;
  }

  if (lang === "section") {
    const section = parseSectionBody(body);
    if (!section) return match;
    return `\n${placeholder("section", section)}\n`;
  }

  if (lang === "chips" || lang === "chip_group") {
    const chips = parseChipsBody(body);
    if (chips.length === 0) return match;
    return `\n${placeholder("chips", { chips })}\n`;
  }

  if (lang === "media") {
    const media = parseMediaBody(body);
    if (!media) return match;
    return `\n${placeholder("media", media)}\n`;
  }

  if (lang === "cite") {
    const cite = parseCiteBody(body);
    if (!cite) return match;
    return `\n${placeholder("cite", cite)}\n`;
  }

  if (lang === "compare") {
    const compare = parseCompareBody(body);
    if (!compare) return match;
    return `\n${placeholder("compare", compare)}\n`;
  }

  if (lang === "plan") {
    const plan = parsePlanBody(body);
    if (!plan) return match;
    return `\n${placeholder("plan", plan)}\n`;
  }

  if (lang === "timeline") {
    const timeline = parseTimelineBody(body);
    if (!timeline) return match;
    return `\n${placeholder("timeline", timeline)}\n`;
  }

  if (lang === "shortlist") {
    const shortlist = parseShortlistBody(body);
    if (!shortlist) return match;
    return `\n${placeholder("shortlist", shortlist)}\n`;
  }

  if (lang === "decision") {
    const decision = parseDecisionBody(body);
    if (!decision) return match;
    return `\n${placeholder("decision", decision)}\n`;
  }

  if (lang === "brief") {
    const brief = parseBriefBody(body);
    if (!brief) return match;
    return `\n${placeholder("brief", brief)}\n`;
  }

  if (lang === "dashboard") {
    const dashboard = parseDashboardBody(body);
    if (!dashboard) return match;
    return `\n${placeholder("dashboard", dashboard)}\n`;
  }

  if (lang === "report") {
    const report = parseReportBody(body);
    if (!report) return match;
    return `\n${placeholder("report", report)}\n`;
  }

  if (lang === "slides") {
    const deck = parseSlidesDeck(body);
    if (!deck) return match;
    const slides: LiquidSlidesProps = {
      slides: deck.slides.map((s) => {
        const item: LiquidSlideItem = {
          id: s.id,
          label: s.label,
          layout: s.layout,
          body: s.body,
        };
        if (s.bg) item.bg = s.bg;
        if (s.scrim) item.scrim = s.scrim;
        return item;
      }),
      columns: deck.columns,
      theme: deck.theme,
    };
    if (deck.title) slides.title = deck.title;
    return `\n${placeholder("slides", slides)}\n`;
  }

  if (lang === "chart") {
    const chart = parseChartBody(body);
    if (!chart) return match;
    const chartIndex = chartEditIndexAt(fullSource, matchIndex);
    return `\n${placeholder("chart", chart, chartIndex)}\n`;
  }

  if (lang === "tabs") {
    const tabs = parseTabsBody(body);
    if (!tabs) return match;
    return `\n${placeholder("tabs", tabs)}\n`;
  }

  if (lang === "steps") {
    const steps = parseStepsBody(body);
    if (!steps) return match;
    return `\n${placeholder("steps", steps)}\n`;
  }

  if (lang === "accordion") {
    const accordion = parseAccordionBody(body);
    if (!accordion) return match;
    return `\n${placeholder("accordion", accordion)}\n`;
  }

  if (lang === "tree") {
    const tree = parseTreeBody(body);
    if (!tree) return match;
    return `\n${placeholder("tree", tree)}\n`;
  }

  if (lang === "kanban") {
    const columns = parseKanbanColumnsFromBody(body);
    const cols = columns
      .map((column) => {
        const cards = column.cards
          .map(
            (card) =>
              `<div class="liquid-mini-kanban__card">${escapeHtml(card.text || "Card")}</div>`,
          )
          .join("");
        return `<div class="liquid-mini-kanban__column"><p class="liquid-mini-kanban__column-title">${escapeHtml(column.title)}</p><div class="liquid-mini-kanban__cards">${cards}</div></div>`;
      })
      .join("");
    return `\n<div class="liquid-mini-kanban" data-liquid-static="kanban"><p class="liquid-mini-kanban__label">Board</p><div class="liquid-mini-kanban__board">${cols}</div></div>\n`;
  }

  return match;
}

/**
 * Replace Liquid fences + `{{icon:name}}` with sanitize-safe placeholders.
 *
 * Nested liquid fences (```cite inside ```brief) are resolved innermost-first
 * so an outer fence does not close on an inner fence's backticks.
 *
 * Documentation fences (bare / markdown / python / …) are protected first so
 * example ```tabs inside them stay opaque text instead of becoming placeholder HTML.
 *
 * Mistaken prose ```code / bare ``` fences unwrap; trailing unclosed prose
 * openers are stripped.
 */
export function preprocessLiquidEmbeds(source: string): string {
  const normalized = source.replace(/\r\n/g, "\n");
  // Soft-convert invented "| Source: … |" lines into italic whispers
  let out = normalized.replace(
    /^\|\s*Source:\s*(.+?)\s*\|\s*$/gim,
    (_m, src: string) => `*Source: ${String(src).trim()}*`,
  );

  const protectedSlots: string[] = [];
  out = shieldNonLiquidDocumentationFences(out, protectedSlots);
  out = processNestedLiquidFences(out);
  out = restoreProtectedFences(out, protectedSlots);
  out = repairTrailingUnclosedProseFence(out);

  // Inline icons — skip inside remaining fences
  const lines = out.split("\n");
  const result: string[] = [];
  let inFence = false;
  for (const line of lines) {
    const trimmed = line.trimStart();
    if (trimmed.startsWith("```")) {
      inFence = !inFence;
      result.push(line);
      continue;
    }
    if (inFence) {
      result.push(line);
      continue;
    }
    result.push(
      line.replace(/\{\{icon:([a-zA-Z0-9_-]+)\}\}/g, (_m, name: string) => {
        const id = normalizeIconId(name);
        if (!id) return "";
        return `<span class="liquid-md-icon" data-liquid-icon="${escapeAttr(id)}" aria-hidden="true"></span>`;
      }),
    );
  }
  return result.join("\n");
}

interface TopLevelFence {
  openLine: number;
  closeLine: number;
  lang: string;
  body: string;
  raw: string;
}

function matchFenceOpen(line: string): { lang: string; ticks: number } | null {
  const m = line.match(/^(````*)([a-zA-Z0-9_-]*)[ \t]*$/);
  if (!m || m[1]!.length < 3) return null;
  return { ticks: m[1]!.length, lang: (m[2] ?? "").toLowerCase() };
}

function matchFenceClose(line: string, openTicks: number): boolean {
  // CommonMark: closer has no info string and at least as many backticks.
  const m = line.match(/^(````*)[ \t]*$/);
  if (!m || m[1]!.length < openTicks) return false;
  return true;
}

function scanTopLevelFences(source: string): TopLevelFence[] {
  const lines = source.split("\n");
  const fences: TopLevelFence[] = [];
  let i = 0;
  while (i < lines.length) {
    const open = matchFenceOpen(lines[i]!);
    if (!open) {
      i += 1;
      continue;
    }
    // Nest-aware close: ```report … ```chart … ``` … ``` must not treat the
    // inner chart closer as the report closer (that orphaned the real closer +
    // following sibling fences into a shielded "docs" slot).
    let depth = 1;
    let j = i + 1;
    while (j < lines.length && depth > 0) {
      const nestedOpen = matchFenceOpen(lines[j]!);
      if (nestedOpen && nestedOpen.ticks >= open.ticks && nestedOpen.lang) {
        depth += 1;
        j += 1;
        continue;
      }
      if (matchFenceClose(lines[j]!, open.ticks)) {
        depth -= 1;
        if (depth === 0) break;
      }
      j += 1;
    }
    if (j >= lines.length || depth !== 0) {
      break;
    }
    const body = lines.slice(i + 1, j).join("\n");
    let closeLine = j;
    // Authors often wrap examples as ```\n```tabs\n...\n```\n``` — the final
    // bare fence is a redundant outer closer. Drop it so it cannot open a new
    // code block over trailing prose.
    const isLiquid = Boolean(open.lang && LIQUID_FENCE_LANGS.has(open.lang));
    if (
      !isLiquid &&
      bodyContainsFenceOpener(body) &&
      j + 1 < lines.length &&
      matchFenceClose(lines[j + 1]!, open.ticks)
    ) {
      closeLine = j + 1;
    }
    fences.push({
      openLine: i,
      closeLine,
      lang: open.lang,
      body,
      raw: lines.slice(i, closeLine + 1).join("\n"),
    });
    i = closeLine + 1;
  }
  return fences;
}

function bodyContainsFenceOpener(body: string): boolean {
  return /^````*[a-zA-Z0-9_-]*[ \t]*$/m.test(body);
}

const DOC_FENCE_SLOT_RE = /\uE000LIQUID_DOC_FENCE_(\d+)\uE000/g;

/**
 * Shield non-liquid fences so innermost-first liquid rewriting cannot eat
 * ```tabs examples inside documentation code blocks.
 */
function shieldNonLiquidDocumentationFences(
  source: string,
  slots: string[],
): string {
  const fences = scanTopLevelFences(source);
  if (fences.length === 0) return source;

  const lines = source.split("\n");
  for (let i = fences.length - 1; i >= 0; i--) {
    const fence = fences[i]!;
    const isLiquid = Boolean(fence.lang && LIQUID_FENCE_LANGS.has(fence.lang));
    if (isLiquid) continue;

    const mistakenProse =
      (!fence.lang || PROSE_MISTAKEN_FENCE_LANGS.has(fence.lang)) &&
      !bodyContainsFenceOpener(fence.body) &&
      looksLikeProseNotCode(fence.body);

    if (mistakenProse) {
      const bodyLines = fence.body.length > 0 ? fence.body.split("\n") : [];
      lines.splice(fence.openLine, fence.closeLine - fence.openLine + 1, ...bodyLines);
      continue;
    }

    const token = `\uE000LIQUID_DOC_FENCE_${slots.length}\uE000`;
    slots.push(fence.raw);
    lines.splice(fence.openLine, fence.closeLine - fence.openLine + 1, token);
  }
  return lines.join("\n");
}

function restoreProtectedFences(source: string, slots: string[]): string {
  if (slots.length === 0) return source;
  return source.replace(DOC_FENCE_SLOT_RE, (_m, index: string) => {
    const slot = slots[Number(index)];
    return slot ?? "";
  });
}

/**
 * Innermost-first liquid rewrite (nest hosts + top-level liquid leaves).
 * Replace one fence per pass. Global multi-match String#replace breaks when a
 * nested chart and a later sibling card both match — the report host never
 * rewrites and the card stays raw.
 */
function processNestedLiquidFences(body: string): string {
  const fenceReSource =
    "^```([a-zA-Z0-9_-]*)[ \\t]*\\n((?:(?!^```)[\\s\\S])*?)^```[ \\t]*$";
  let out = body;
  for (let pass = 0; pass < 64; pass++) {
    const fenceRe = new RegExp(fenceReSource, "gm");
    const hits: { index: number; text: string; lang: string; inner: string }[] =
      [];
    let match: RegExpExecArray | null;
    while ((match = fenceRe.exec(out)) !== null) {
      hits.push({
        index: match.index,
        text: match[0],
        lang: match[1] ?? "",
        inner: match[2] ?? "",
      });
    }
    let replacedOne = false;
    for (const hit of hits) {
      const replaced = replaceLiquidFenceMatch(
        hit.text,
        hit.lang,
        hit.inner,
        out,
        hit.index,
      );
      if (replaced === hit.text) continue;
      out =
        out.slice(0, hit.index) +
        replaced +
        out.slice(hit.index + hit.text.length);
      replacedOne = true;
      break;
    }
    if (!replacedOne) break;
  }
  return out;
}
