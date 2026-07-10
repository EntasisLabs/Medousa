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
  | "media";

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
