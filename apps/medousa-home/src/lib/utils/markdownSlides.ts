/**
 * Vault slides — `kind: slides` / `medousa-deck` notes + ```slides fence grammar.
 * KV preamble + nest-aware `---` labeled sections.
 */

import {
  normalizeKind,
  serializeFrontmatter,
  stripFrontmatter,
} from "$lib/utils/vaultFrontmatter";

export type SlideLayout = "hero" | "split" | "stack";
export type SlideWash = "paper" | "dusk" | "ink" | "mist" | "ember";
export type SlideScrim = "dark" | "light" | "none";
export type SlideMotion = "none" | "fade" | "fade-up";

/** Declarative positioned graphic on the 16:9 stage (normalized 0–1). */
export interface SlideLayer {
  id: string;
  src: string;
  x: number;
  y: number;
  w: number;
  h?: number;
}

export interface SlideSection {
  id: string;
  label: string;
  layout: SlideLayout;
  body: string;
  /** Named wash or image path/URL. Empty = inherit deck theme. */
  bg?: string;
  scrim?: SlideScrim;
  layers?: SlideLayer[];
  motion?: SlideMotion;
  /** Speaker notes (player chrome; not shown on slide face). */
  notes?: string;
}

export type SlidesDeckWidth = "narrow" | "medium" | "wide" | "full";

export interface SlidesDeck {
  title: string;
  theme: string;
  columns: "1" | "2" | "3";
  /** Optional Live / Preview paper width for the deck surface. */
  width?: SlidesDeckWidth;
  slides: SlideSection[];
}

const SLIDE_LAYOUTS = new Set<SlideLayout>(["hero", "split", "stack"]);
export const SLIDE_WASHES = new Set<SlideWash>([
  "paper",
  "dusk",
  "ink",
  "mist",
  "ember",
]);
const SLIDE_SCRIMS = new Set<SlideScrim>(["dark", "light", "none"]);
const SLIDE_MOTIONS = new Set<SlideMotion>(["none", "fade", "fade-up"]);

export function normalizeSlideMotion(value: string | undefined): SlideMotion {
  const v = (value ?? "none").trim().toLowerCase();
  return SLIDE_MOTIONS.has(v as SlideMotion) ? (v as SlideMotion) : "none";
}

function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.min(1, Math.max(0, n));
}

function parseCoord(raw: string | undefined, fallback: number): number {
  if (raw === undefined || raw === "") return fallback;
  const n = Number(raw);
  return clamp01(Number.isFinite(n) ? n : fallback);
}

/** Dark washes use light ink; light washes use dark ink. */
export const SLIDE_WASH_INK: Record<SlideWash, "dark" | "light"> = {
  paper: "dark",
  mist: "dark",
  dusk: "light",
  ink: "light",
  ember: "light",
};

export function isSlideWash(value: string): value is SlideWash {
  return SLIDE_WASHES.has(value.trim().toLowerCase() as SlideWash);
}

export function normalizeSlideWash(value: string | undefined): SlideWash {
  const v = (value ?? "paper").trim().toLowerCase();
  return isSlideWash(v) ? v : "paper";
}

/** True when bg is a vault path or remote URL (not a named wash). */
export function isSlideBgImage(bg: string | undefined): boolean {
  const v = (bg ?? "").trim();
  if (!v || isSlideWash(v)) return false;
  return (
    v.startsWith("./") ||
    v.startsWith("../") ||
    v.startsWith("/") ||
    /^https?:\/\//i.test(v) ||
    /\.(png|jpe?g|gif|webp|svg|avif)(\?.*)?$/i.test(v)
  );
}

export function normalizeSlideScrim(
  value: string | undefined,
  bgIsImage: boolean,
): SlideScrim | undefined {
  const v = (value ?? "").trim().toLowerCase();
  if (v && SLIDE_SCRIMS.has(v as SlideScrim)) return v as SlideScrim;
  // Image backgrounds default to full photo brightness (no overlay).
  if (bgIsImage) return "none";
  return undefined;
}

function matchFenceOpen(line: string): { ticks: number; lang: string } | null {
  const m = /^(`{3,})([^\s`]*)/.exec(line);
  if (!m) return null;
  return { ticks: m[1]!.length, lang: (m[2] ?? "").trim().toLowerCase() };
}

function matchFenceClose(line: string, ticks: number): boolean {
  return new RegExp(`^\`{${ticks},}\\s*$`).test(line);
}

/** Split on `---` that are outside fenced code blocks. */
export function splitTopLevelSectionBreaks(source: string): string[] {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const parts: string[] = [];
  let buf: string[] = [];
  let fenceTicks = 0;

  for (const line of lines) {
    if (fenceTicks > 0) {
      buf.push(line);
      if (matchFenceClose(line, fenceTicks)) fenceTicks = 0;
      continue;
    }
    const open = matchFenceOpen(line);
    if (open?.lang) {
      fenceTicks = open.ticks;
      buf.push(line);
      continue;
    }
    if (/^---\s*$/.test(line.trim())) {
      parts.push(buf.join("\n"));
      buf = [];
      continue;
    }
    buf.push(line);
  }
  parts.push(buf.join("\n"));
  return parts;
}

function parseKvPreamble(block: string): {
  fields: Record<string, string>;
  body: string;
} {
  const lines = block.replace(/\r\n/g, "\n").split("\n");
  const fields: Record<string, string> = {};
  let i = 0;
  for (; i < lines.length; i++) {
    const stripped = (lines[i] ?? "").trim();
    if (!stripped) {
      if (Object.keys(fields).length > 0) {
        i += 1;
        break;
      }
      continue;
    }
    if (/^[a-zA-Z][a-zA-Z0-9_-]*\s*:/.test(stripped) && !stripped.startsWith("|")) {
      const colon = stripped.indexOf(":");
      const key = stripped.slice(0, colon).trim().toLowerCase();
      const value = stripped.slice(colon + 1).trim();
      if (key && value) fields[key] = value;
      continue;
    }
    break;
  }
  return { fields, body: lines.slice(i).join("\n").trim() };
}

/** Slide KV + nested `layer:` blocks (indented props). */
function parseSlideSectionBlock(block: string): {
  fields: Record<string, string>;
  layers: SlideLayer[];
  body: string;
} {
  const lines = block.replace(/\r\n/g, "\n").split("\n");
  const fields: Record<string, string> = {};
  const layers: SlideLayer[] = [];
  let i = 0;

  while (i < lines.length) {
    const raw = lines[i] ?? "";
    const stripped = raw.trim();
    if (!stripped) {
      if (Object.keys(fields).length > 0 || layers.length > 0) {
        i += 1;
        break;
      }
      i += 1;
      continue;
    }
    if (stripped.startsWith("|") || stripped.startsWith("#") || stripped.startsWith("```")) {
      break;
    }
    if (!/^[a-zA-Z][a-zA-Z0-9_-]*\s*:/.test(stripped)) break;

    const colon = stripped.indexOf(":");
    const key = stripped.slice(0, colon).trim().toLowerCase();
    const value = stripped.slice(colon + 1).trim();

    if (key === "layer") {
      const id = value || `layer-${layers.length + 1}`;
      const props: Record<string, string> = {};
      i += 1;
      while (i < lines.length) {
        const nested = lines[i] ?? "";
        if (!/^\s+\S/.test(nested)) break;
        const nestTrim = nested.trim();
        if (!/^[a-zA-Z][a-zA-Z0-9_-]*\s*:/.test(nestTrim)) break;
        const nc = nestTrim.indexOf(":");
        const nk = nestTrim.slice(0, nc).trim().toLowerCase();
        const nv = nestTrim.slice(nc + 1).trim();
        if (nk) props[nk] = nv;
        i += 1;
      }
      const src = (props.src ?? "").trim();
      if (src) {
        layers.push({
          id,
          src,
          x: parseCoord(props.x, 0),
          y: parseCoord(props.y, 0),
          w: parseCoord(props.w, 0.2),
          ...(props.h !== undefined && props.h !== ""
            ? { h: parseCoord(props.h, 0.2) }
            : {}),
        });
      }
      continue;
    }

    if (key && value) fields[key] = value;
    i += 1;
  }

  return { fields, layers, body: lines.slice(i).join("\n").trim() };
}

function slugSlideId(label: string, index: number): string {
  const base = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
  return base || `slide-${index + 1}`;
}

function normalizeLayout(raw: string | undefined): SlideLayout {
  const v = (raw ?? "split").trim().toLowerCase();
  return SLIDE_LAYOUTS.has(v as SlideLayout) ? (v as SlideLayout) : "split";
}

/** Parse slides fence/body grammar into a deck model. */
export function parseSlidesDeck(body: string): SlidesDeck | null {
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;

  const parts = splitTopLevelSectionBreaks(normalized);
  const preamble = parseKvPreamble(parts[0] ?? "");
  const sectionParts = parts.slice(1);

  // Allow a body-only deck with ## Slide headings as a soft fallback when no --- sections.
  let slides: SlideSection[] = [];
  if (sectionParts.length === 0) {
    const fallbackBody = preamble.body || normalized;
    if (!fallbackBody.trim() && !preamble.fields.title) return null;
    if (fallbackBody.trim()) {
      slides = [
        {
          id: "slide-1",
          label: preamble.fields.title?.trim() || "Slide",
          layout: "stack",
          body: fallbackBody.trim(),
        },
      ];
    }
  } else {
    for (let i = 0; i < sectionParts.length; i++) {
      const parsed = parseSlideSectionBlock(sectionParts[i] ?? "");
      const label =
        (parsed.fields.label ?? parsed.fields.title)?.trim() || `Slide ${i + 1}`;
      const slideBody = parsed.body;
      if (!label && !slideBody) continue;
      const bg = parsed.fields.bg?.trim();
      const bgIsImage = isSlideBgImage(bg);
      const scrim = normalizeSlideScrim(parsed.fields.scrim, bgIsImage);
      const motion = normalizeSlideMotion(parsed.fields.motion);
      const notes = parsed.fields.notes?.trim();
      const slide: SlideSection = {
        id: slugSlideId(label, i),
        label,
        layout: normalizeLayout(parsed.fields.layout),
        body: slideBody,
      };
      if (bg) slide.bg = bg;
      if (scrim && (bgIsImage || parsed.fields.scrim?.trim())) {
        slide.scrim = scrim;
      }
      if (parsed.layers.length > 0) slide.layers = parsed.layers;
      if (motion !== "none") slide.motion = motion;
      if (notes) slide.notes = notes;
      slides.push(slide);
    }
  }

  if (slides.length < 1) return null;

  const columnsRaw = (preamble.fields.columns ?? "2").trim();
  const columns =
    columnsRaw === "1" || columnsRaw === "3" ? columnsRaw : "2";

  const widthRaw = (preamble.fields.width ?? "").trim().toLowerCase();
  const width =
    widthRaw === "narrow" ||
    widthRaw === "medium" ||
    widthRaw === "wide" ||
    widthRaw === "full"
      ? (widthRaw as SlidesDeckWidth)
      : undefined;

  return {
    title: preamble.fields.title?.trim() ?? "",
    theme: normalizeSlideWash(preamble.fields.theme),
    columns,
    ...(width ? { width } : {}),
    slides,
  };
}

/** Serialize a deck model back to fence-inner markdown (no outer ```). */
export function serializeSlidesDeckBody(deck: SlidesDeck): string {
  const lines: string[] = [];
  if (deck.title.trim()) lines.push(`title: ${deck.title.trim()}`);
  if (deck.theme.trim() && deck.theme.trim() !== "paper") {
    lines.push(`theme: ${deck.theme.trim()}`);
  } else if (deck.theme.trim()) {
    lines.push(`theme: paper`);
  }
  lines.push(`columns: ${deck.columns}`);
  if (deck.width && deck.width !== "wide") {
    lines.push(`width: ${deck.width}`);
  }
  lines.push("");

  for (const slide of deck.slides) {
    lines.push("---");
    lines.push(`label: ${slide.label}`);
    lines.push(`layout: ${slide.layout}`);
    if (slide.bg?.trim()) lines.push(`bg: ${slide.bg.trim()}`);
    // Default for images is `none` — only serialize when explicit non-default.
    if (slide.scrim && !(isSlideBgImage(slide.bg) && slide.scrim === "none")) {
      lines.push(`scrim: ${slide.scrim}`);
    }
    if (slide.motion && slide.motion !== "none") {
      lines.push(`motion: ${slide.motion}`);
    }
    if (slide.notes?.trim()) lines.push(`notes: ${slide.notes.trim()}`);
    for (const layer of slide.layers ?? []) {
      lines.push(`layer: ${layer.id}`);
      lines.push(`  src: ${layer.src}`);
      lines.push(`  x: ${layer.x}`);
      lines.push(`  y: ${layer.y}`);
      lines.push(`  w: ${layer.w}`);
      if (layer.h !== undefined) lines.push(`  h: ${layer.h}`);
    }
    lines.push("");
    if (slide.body.trim()) lines.push(slide.body.trim());
    lines.push("");
  }

  return lines.join("\n").replace(/\n+$/, "\n");
}

export function serializeSlidesFence(deck: SlidesDeck): string {
  return "```slides\n" + serializeSlidesDeckBody(deck) + "```\n";
}

export function readMedousaDeckKind(markdown: string): string | null {
  const { frontmatter } = stripFrontmatter(markdown);
  if (!frontmatter) return null;
  for (const line of frontmatter.split("\n")) {
    const match = line.match(/^medousa-deck:\s*(.+)$/i);
    if (match) return match[1]!.trim();
  }
  return null;
}

export function noteHasSlidesDeck(markdown: string): boolean {
  if (readMedousaDeckKind(markdown)) return true;
  const { frontmatter } = stripFrontmatter(markdown);
  if (!frontmatter) return false;
  for (const line of frontmatter.split("\n")) {
    if (!line.trimStart().startsWith("kind:")) continue;
    const value = line.slice(line.indexOf(":") + 1);
    return normalizeKind(value) === "slides";
  }
  return false;
}

export function slidesDeckFromContent(markdown: string): SlidesDeck {
  const { content } = stripFrontmatter(markdown);
  return (
    parseSlidesDeck(content) ?? {
      title: "",
      theme: "paper",
      columns: "2",
      slides: [
        {
          id: "slide-1",
          label: "Title",
          layout: "hero",
          body: "# New deck\n\nAdd slide content here.",
        },
      ],
    }
  );
}

export function replaceSlidesDeck(markdown: string, deck: SlidesDeck): string {
  const { frontmatter } = stripFrontmatter(markdown);
  const body = serializeSlidesDeckBody(deck).trimEnd() + "\n";
  const lines = (frontmatter ?? "")
    .split("\n")
    .map((l) => l.trimEnd())
    .filter((l) => l.length > 0);
  const ensured: string[] = [];
  let hasKind = false;
  let hasDeck = false;
  for (const line of lines) {
    if (line.trimStart().toLowerCase().startsWith("kind:")) {
      ensured.push("kind: slides");
      hasKind = true;
      continue;
    }
    if (line.trimStart().toLowerCase().startsWith("medousa-deck:")) {
      ensured.push("medousa-deck: basic");
      hasDeck = true;
      continue;
    }
    ensured.push(line);
  }
  if (!hasKind) ensured.unshift("kind: slides");
  if (!hasDeck) ensured.push("medousa-deck: basic");
  return serializeFrontmatter(ensured.join("\n"), body);
}

export function createEmptySlidesNote(title = "Untitled deck"): string {
  const deck: SlidesDeck = {
    title,
    theme: "paper",
    columns: "2",
    slides: [
      {
        id: "title",
        label: "Title",
        layout: "hero",
        body: `# ${title}\n\nOne pick for Live polish`,
      },
      {
        id: "story",
        label: "Story",
        layout: "split",
        body: "Prose wraps beside figures…",
      },
    ],
  };
  return serializeFrontmatter(
    "kind: slides\nmedousa-deck: basic",
    serializeSlidesDeckBody(deck).trimEnd() + "\n",
  );
}
