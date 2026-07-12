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
  "brief",
  "dashboard",
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
  | "dashboard";

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

/** Build a card from a KV/point block (carousel item or single card fence). */
function parseCardBlock(block: string): LiquidCardProps | null {
  const fields: Record<string, string> = {};
  const points: LiquidCardPoint[] = [];

  for (const raw of block.split("\n")) {
    const line = stripFenceLineChrome(raw);
    if (!line || line.startsWith("#") || line === "---") continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (!key || !value) continue;
    if (key === "point" || key === "points") {
      const point = parsePointValue(value);
      if (point) points.push(point);
      continue;
    }
    // chips/badges keep pipes; other keys take first assignment
    if (key === "chips" || key === "badges") {
      fields[key] = fields[key] ? `${fields[key]} | ${value}` : value;
      continue;
    }
    if (!(key in fields)) fields[key] = value;
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

/** Collect multi-line KV fields (title: / body: …) until a blank or separator. */
function parseKvBlock(body: string): Record<string, string> {
  const fields: Record<string, string> = {};
  for (const raw of body.split("\n")) {
    const line = stripFenceLineChrome(raw);
    if (!line || line.startsWith("#") || line === "---") continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (!key || !value) continue;
    // Pipe-separated multi-kv: "title: Sol | body: Flagship"
    // vs value that merely contains a pipe: "title: Albums | AllMusic"
    if (line.includes("|") && /\|\s*[a-zA-Z][a-zA-Z0-9_-]*\s*:/.test(line)) {
      Object.assign(fields, parseKvLine(line));
      continue;
    }
    fields[key] = value;
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

const BRIEF_TONES = new Set(["research", "brief", "memo"]);

function stripAtxHeading(line: string): string | null {
  const m = line.trim().match(/^#{1,6}\s+(.+?)(?:\s+#*)?$/);
  if (!m) return null;
  return m[1].replace(/\s+#*$/, "").trim() || null;
}

/** Split a markdown blob on ATX headings into brief sections. */
function parseAtxBriefSections(block: string, startIndex: number): LiquidBriefSection[] {
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
      id: slugCompareId(heading, "section", startIndex + sections.length),
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
        id: slugCompareId(heading, "section", index),
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
  const fields = parseDecisionOptionFields(block);
  const title = (fields.title ?? fields.label)?.trim();
  if (!title) return null;
  const src: LiquidBriefSource = {
    id: slugCompareId(title, "source", index),
    title,
  };
  if (fields.url) src.url = fields.url;
  if (fields.quote) src.quote = fields.quote;
  else if (fields.body) src.quote = fields.body;
  return src;
}

function parseBriefBody(body: string): LiquidBriefProps | null {
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
          id: slugCompareId(pendingHeading, "section", sections.length),
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
            id: slugCompareId(heading, "section", 0),
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
      id: slugCompareId(label, "tile", tiles.length),
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
 */
function repairTrailingUnclosedProseFence(source: string): string {
  const lines = source.split("\n");
  let lastFence = -1;
  let fenceCount = 0;
  for (let i = 0; i < lines.length; i++) {
    if (/^```[a-zA-Z0-9_-]*[ \t]*$/.test(lines[i])) {
      lastFence = i;
      fenceCount++;
    }
  }
  // Even count ⇒ every opener already has a closer (incl. a trailing ```).
  if (lastFence < 0 || fenceCount % 2 === 0) return source;
  const after = lines.slice(lastFence + 1).join("\n");
  if (/^```/m.test(after)) return source;
  const lang = lines[lastFence].slice(3).trim().toLowerCase();
  if (lang && LIQUID_FENCE_LANGS.has(lang)) return source;
  if (!after.trim()) {
    return lines.slice(0, lastFence).join("\n");
  }
  if (
    lang === "" ||
    PROSE_MISTAKEN_FENCE_LANGS.has(lang) ||
    looksLikeProseNotCode(after)
  ) {
    return [...lines.slice(0, lastFence), ...lines.slice(lastFence + 1)].join("\n");
  }
  return source;
}

function replaceLiquidFenceMatch(match: string, langRaw: string, body: string): string {
  const lang = langRaw.trim().toLowerCase();

  // Models wrap final statements in ```code / bare ``` — unwrap when clearly prose
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

  return match;
}

/**
 * Replace Liquid fences + `{{icon:name}}` with sanitize-safe placeholders.
 *
 * Nested liquid fences (```cite inside ```brief) are resolved innermost-first
 * so an outer fence does not close on an inner fence's backticks.
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

  // Body must not contain a nested fence line — forces innermost-first matching.
  // Lang optional — bare ``` … ``` often wraps final statements.
  const fenceRe = /^```([a-zA-Z0-9_-]*)[ \t]*\n((?:(?!^```)[\s\S])*?)^```[ \t]*$/gm;

  for (let pass = 0; pass < 12; pass++) {
    const next = out.replace(fenceRe, replaceLiquidFenceMatch);
    if (next === out) break;
    out = next;
  }

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
