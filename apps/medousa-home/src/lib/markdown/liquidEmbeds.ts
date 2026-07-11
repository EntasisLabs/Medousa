/**
 * Liquid markdown embeds — preprocess fences + icon shortcodes into inert
 * placeholders the renderer sanitizes and hydrateLiquidEmbeds mounts.
 *
 * Same pattern as mermaid / wikilinks: model writes familiar markdown; runtime
 * owns the component vocabulary.
 */

import { escapeAttr } from "./escape";

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
  | "decision";

export interface LiquidCardProps {
  title: string;
  subtitle?: string;
  body?: string;
  emoji?: string;
  image?: string;
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

export interface LiquidCompareProps {
  title?: string;
  subtitle?: string;
  recommendation?: string;
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

export interface LiquidSectionProps {
  title: string;
  subtitle?: string;
  body?: string;
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
  return Buffer.from(json, "utf8").toString("base64");
}

export function decodeLiquidProps<T = unknown>(encoded: string): T | null {
  try {
    let json: string;
    if (typeof atob === "function") {
      json = decodeURIComponent(escape(atob(encoded)));
    } else {
      json = Buffer.from(encoded, "base64").toString("utf8");
    }
    return JSON.parse(json) as T;
  } catch {
    return null;
  }
}

function placeholder(kind: LiquidEmbedKind, props: unknown): string {
  return `<div class="liquid-md-embed" data-liquid-embed="${escapeAttr(kind)}" data-liquid-props="${escapeAttr(encodeProps(props))}"></div>`;
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

function parseCardBody(body: string): LiquidCardProps | null {
  const fields: Record<string, string> = {};
  for (const raw of body.split("\n")) {
    const line = stripFenceLineChrome(raw);
    if (!line || line.startsWith("#")) continue;
    // Multi-line kv: "title: Sol"
    const colon = line.indexOf(":");
    if (colon > 0 && !line.includes("|")) {
      const key = line.slice(0, colon).trim().toLowerCase();
      const value = line.slice(colon + 1).trim();
      if (key && value) fields[key] = value;
      continue;
    }
    Object.assign(fields, parseKvLine(line));
  }
  const title = fields.title?.trim();
  if (!title) return null;
  const card: LiquidCardProps = { title };
  if (fields.subtitle) card.subtitle = fields.subtitle;
  if (fields.body) card.body = fields.body;
  if (fields.emoji) card.emoji = fields.emoji;
  if (fields.image) card.image = fields.image;
  return card;
}

function parseCarouselBody(body: string): LiquidCardProps[] {
  const cards: LiquidCardProps[] = [];
  for (const raw of body.split("\n")) {
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

/** Collect multi-line KV fields (title: / body: …) until a blank or separator. */
function parseKvBlock(body: string): Record<string, string> {
  const fields: Record<string, string> = {};
  for (const raw of body.split("\n")) {
    const line = stripFenceLineChrome(raw);
    if (!line || line.startsWith("#") || line === "---") continue;
    const colon = line.indexOf(":");
    if (colon > 0 && !line.includes("|")) {
      const key = line.slice(0, colon).trim().toLowerCase();
      const value = line.slice(colon + 1).trim();
      if (key && value) fields[key] = value;
      continue;
    }
    Object.assign(fields, parseKvLine(line));
  }
  return fields;
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

function slugCompareId(label: string, prefix: string, index: number): string {
  const base = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return base ? `${prefix}-${base}` : `${prefix}-${index}`;
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
    id: slugCompareId(label, "entity", i),
    label,
    values: {},
  }));

  const axes: LiquidCompareAxis[] = [];
  for (let r = 1; r < rows.length; r++) {
    const cells = rows[r];
    const axisLabel = (cells[0] ?? "").trim();
    if (!axisLabel) continue;
    const axisId = slugCompareId(axisLabel, "axis", axes.length);
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
  return compare;
}

const PLAN_GROUPINGS = new Set(["day", "phase", "milestone"]);

function parsePlanBody(body: string): LiquidPlanProps | null {
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
      id: slugCompareId(label, "segment", segments.length),
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
      id: slugCompareId(label, "event", events.length),
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
      id: slugCompareId(label, "item", items.length),
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
      id: slugCompareId(label, "option", options.length),
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

function normalizeIconId(raw: string): string | null {
  const id = raw.trim().toLowerCase().replace(/_/g, "-");
  if (!id || !LIQUID_ICON_ALLOWLIST.has(id)) return null;
  // Canonical kebab form for data attribute
  return id.replace(/messagecircle/, "message-circle")
    .replace(/filecode/, "file-code")
    .replace(/alerttriangle/, "alert-triangle");
}

/**
 * Replace Liquid fences + `{{icon:name}}` with sanitize-safe placeholders.
 * Unknown fence langs are left untouched.
 */
export function preprocessLiquidEmbeds(source: string): string {
  const normalized = source.replace(/\r\n/g, "\n");
  const fenceRe = /^```([a-zA-Z0-9_-]+)[ \t]*\n([\s\S]*?)^```[ \t]*$/gm;

  let out = normalized.replace(fenceRe, (match, langRaw: string, body: string) => {
    const lang = langRaw.trim().toLowerCase();
    if (!LIQUID_FENCE_LANGS.has(lang)) return match;

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

    return match;
  });

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
