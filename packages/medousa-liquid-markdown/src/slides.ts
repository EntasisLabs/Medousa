export type SlideLayout = "hero" | "split" | "stack";
export type SlideScrim = "dark" | "light" | "none";
export type SlideMotion = "none" | "fade" | "fade-up";

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
  bg?: string;
  scrim?: SlideScrim;
  layers?: SlideLayer[];
  motion?: SlideMotion;
  notes?: string;
}

export interface SlidesDeck {
  title: string;
  theme: string;
  columns: "1" | "2" | "3";
  width?: "narrow" | "medium" | "wide" | "full";
  slides: SlideSection[];
}

const LAYOUTS = new Set<SlideLayout>(["hero", "split", "stack"]);
const SCRIMS = new Set<SlideScrim>(["dark", "light", "none"]);
const MOTIONS = new Set<SlideMotion>(["none", "fade", "fade-up"]);
const WASHES = new Set(["paper", "dusk", "ink", "mist", "ember"]);

function fenceOpen(line: string): number {
  return /^(`{3,})[^\s`]*/.exec(line)?.[1]?.length ?? 0;
}

/** Split on section breaks outside nested Markdown fences. */
export function splitTopLevelSectionBreaks(source: string): string[] {
  const parts: string[] = [];
  let lines: string[] = [];
  let ticks = 0;
  for (const line of source.replace(/\r\n/g, "\n").split("\n")) {
    if (ticks > 0) {
      lines.push(line);
      if (new RegExp("^`{" + ticks + ",}\\s*$").test(line)) ticks = 0;
      continue;
    }
    const opened = fenceOpen(line);
    if (opened > 0) {
      ticks = opened;
      lines.push(line);
      continue;
    }
    if (/^---\s*$/.test(line.trim())) {
      parts.push(lines.join("\n"));
      lines = [];
      continue;
    }
    lines.push(line);
  }
  parts.push(lines.join("\n"));
  return parts;
}

function parsePreamble(block: string): { fields: Record<string, string>; body: string } {
  const source = block.replace(/\r\n/g, "\n").split("\n");
  const fields: Record<string, string> = {};
  let index = 0;
  for (; index < source.length; index += 1) {
    const line = source[index]!.trim();
    if (!line) {
      if (Object.keys(fields).length > 0) {
        index += 1;
        break;
      }
      continue;
    }
    const match = /^([a-zA-Z][a-zA-Z0-9_-]*)\s*:\s*(.*)$/.exec(line);
    if (!match || line.startsWith("|")) break;
    if (match[2]) fields[match[1]!.toLowerCase()] = match[2].trim();
  }
  return { fields, body: source.slice(index).join("\n").trim() };
}

function clamp(value: string | undefined, fallback: number): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.min(1, Math.max(0, parsed));
}

function parseSection(block: string): {
  fields: Record<string, string>;
  layers: SlideLayer[];
  body: string;
} {
  const source = block.replace(/\r\n/g, "\n").split("\n");
  const fields: Record<string, string> = {};
  const layers: SlideLayer[] = [];
  let index = 0;
  while (index < source.length) {
    const raw = source[index]!;
    const line = raw.trim();
    if (!line) {
      if (Object.keys(fields).length > 0 || layers.length > 0) {
        index += 1;
        break;
      }
      index += 1;
      continue;
    }
    const match = /^([a-zA-Z][a-zA-Z0-9_-]*)\s*:\s*(.*)$/.exec(line);
    if (!match || line.startsWith("|") || line.startsWith("#") || line.startsWith("```")) break;
    const key = match[1]!.toLowerCase();
    const value = match[2]!.trim();
    if (key !== "layer") {
      if (value) fields[key] = value;
      index += 1;
      continue;
    }
    const id = value || `layer-${layers.length + 1}`;
    const props: Record<string, string> = {};
    index += 1;
    while (index < source.length && /^\s+\S/.test(source[index]!)) {
      const nested = /^([a-zA-Z][a-zA-Z0-9_-]*)\s*:\s*(.*)$/.exec(source[index]!.trim());
      if (!nested) break;
      props[nested[1]!.toLowerCase()] = nested[2]!.trim();
      index += 1;
    }
    if (props.src) {
      const layer: SlideLayer = {
        id,
        src: props.src,
        x: clamp(props.x, 0),
        y: clamp(props.y, 0),
        w: clamp(props.w, 0.2),
      };
      if (props.h !== undefined && props.h !== "") layer.h = clamp(props.h, 0.2);
      layers.push(layer);
    }
  }
  return { fields, layers, body: source.slice(index).join("\n").trim() };
}

function imageBackground(value: string | undefined): boolean {
  const bg = value?.trim() ?? "";
  return Boolean(
    bg &&
      !WASHES.has(bg.toLowerCase()) &&
      (bg.startsWith(".") || bg.startsWith("/") || /^https?:\/\//i.test(bg) || /\.(png|jpe?g|gif|webp|svg|avif)(\?.*)?$/i.test(bg)),
  );
}

function slug(label: string, index: number): string {
  return (
    label
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 40) || `slide-${index + 1}`
  );
}

/** Parse the portable portion of the Medousa slides fence grammar. */
export function parseSlidesDeck(body: string): SlidesDeck | null {
  const normalized = body.replace(/\r\n/g, "\n").trim();
  if (!normalized) return null;
  const parts = splitTopLevelSectionBreaks(normalized);
  const preamble = parsePreamble(parts[0] ?? "");
  const sectionParts = parts.slice(1);
  const slides: SlideSection[] = [];

  if (sectionParts.length === 0) {
    const fallback = preamble.body || normalized;
    if (fallback.trim()) {
      slides.push({
        id: "slide-1",
        label: preamble.fields.title?.trim() || "Slide",
        layout: "stack",
        body: fallback.trim(),
      });
    }
  } else {
    for (let index = 0; index < sectionParts.length; index += 1) {
      const parsed = parseSection(sectionParts[index] ?? "");
      const label = (parsed.fields.label ?? parsed.fields.title)?.trim() || `Slide ${index + 1}`;
      const layout = LAYOUTS.has(parsed.fields.layout as SlideLayout)
        ? (parsed.fields.layout as SlideLayout)
        : "split";
      const slide: SlideSection = { id: slug(label, index), label, layout, body: parsed.body };
      const bg = parsed.fields.bg?.trim();
      if (bg) slide.bg = bg;
      const scrim = parsed.fields.scrim?.trim().toLowerCase();
      if (scrim && SCRIMS.has(scrim as SlideScrim)) slide.scrim = scrim as SlideScrim;
      else if (imageBackground(bg)) slide.scrim = "none";
      const motion = parsed.fields.motion?.trim().toLowerCase();
      if (motion && motion !== "none" && MOTIONS.has(motion as SlideMotion)) slide.motion = motion as SlideMotion;
      if (parsed.fields.notes?.trim()) slide.notes = parsed.fields.notes.trim();
      if (parsed.layers.length > 0) slide.layers = parsed.layers;
      slides.push(slide);
    }
  }
  if (slides.length === 0) return null;
  const columns = preamble.fields.columns === "1" || preamble.fields.columns === "3" ? preamble.fields.columns : "2";
  const width = preamble.fields.width;
  return {
    title: preamble.fields.title?.trim() ?? "",
    theme: WASHES.has((preamble.fields.theme ?? "paper").toLowerCase())
      ? (preamble.fields.theme ?? "paper").toLowerCase()
      : "paper",
    columns,
    ...(width === "narrow" || width === "medium" || width === "wide" || width === "full" ? { width } : {}),
    slides,
  };
}
