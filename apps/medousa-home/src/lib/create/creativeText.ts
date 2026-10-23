import { textSegments } from "./creativeRichText";
import { readable } from "svelte/store";
import type { CreativeNode, CreativeTextLayout } from "$lib/types/generated/daemon_api";

export type CreativeText = Extract<CreativeNode["content"], { kind: "text" }>;
export type TextStylePatch = Partial<Pick<CreativeText, "font_family" | "font_size" | "font_weight" | "font_style">> & { text_width?: number | null; text_align?: CreativeTextLayout["align"]; line_height?: number; letter_spacing?: number; paragraph_spacing?: number; underline?: boolean };
export const canvasFonts = [
  { label: "Sans", value: "sans-serif" },
  { label: "Serif", value: "serif" },
  { label: "Mono", value: "monospace" },
  { label: "Handwritten", value: "cursive" },
  { label: "Arial", value: "Arial" },
  { label: "Helvetica", value: "Helvetica" },
  { label: "Georgia", value: "Georgia" },
  { label: "Times New Roman", value: "Times New Roman" },
  { label: "Palatino", value: "Palatino" },
  { label: "Verdana", value: "Verdana" },
  { label: "Trebuchet MS", value: "Trebuchet MS" },
  { label: "Courier New", value: "Courier New" },
  { label: "Menlo", value: "Menlo" },
  { label: "Avenir Next", value: "Avenir Next" },
  { label: "Baskerville", value: "Baskerville" },
] as const;
export const defaultTextStyle = { font_family: "sans-serif", font_size: 32, font_weight: 400, font_style: "normal" } as const;
export function textStyleFor(node: CreativeNode): TextStylePatch {
  return node.content.kind === "text" ? { ...node.content, text_width: node.text_layout?.width ?? null, text_align: node.text_layout?.align ?? "left", line_height: node.text_layout?.line_height ?? 1.3, letter_spacing: node.text_layout?.letter_spacing ?? 0, paragraph_spacing: node.text_layout?.paragraph_spacing ?? 0, underline: node.text_layout?.underline ?? false } : {};
}
export type TextLayout = { align: CreativeTextLayout["align"]; width: number; height: number; insetX: number; padding: number; baseline: number; lineHeight: number; lines: string[]; starts: number[]; ys: number[] };
type LineMetrics = { width: number; actualBoundingBoxLeft?: number; actualBoundingBoxRight?: number; fontBoundingBoxAscent?: number; fontBoundingBoxDescent?: number };
type Measure = (text: string, offset?: number) => LineMetrics;
const layouts = new Map<string, TextLayout>(), baselines = new Map<string, number>();
let cachedCharacters = 0;
let context: CanvasRenderingContext2D | null | undefined;
let fontRevision = 0;
/** Re-measure existing labels if a document font arrives after the canvas opens. */
export const textMetricsVersion = readable(0, set => {
  if (typeof document === "undefined" || !document.fonts) return;
  let disposed = false;
  const refresh = () => {
    if (disposed) return;
    layouts.clear(); baselines.clear(); fontPresence.clear(); cachedCharacters = 0;
    set(++fontRevision);
  };
  document.fonts.addEventListener("loadingdone", refresh);
  void document.fonts.ready.then(refresh);
  return () => { disposed = true; document.fonts.removeEventListener("loadingdone", refresh); };
});
export function textFont(content: CreativeText): string {
  return `${content.font_style ?? "normal"} ${content.font_weight ?? 400} ${content.font_size}px ${content.font_family}`;
}
function measureContext() {
  if (context === undefined && typeof document !== "undefined") {
    try { context = document.createElement("canvas").getContext("2d"); } catch { context = null; }
  }
  return context;
}
/** Read the browser's CSS line box once per font, so SVG and textarea baselines agree. */
function cssBaseline(font: string, lineHeight: number, fallback: number): number {
  const key = `${font}/${lineHeight}`;
  const cached = baselines.get(key);
  if (cached !== undefined) return cached;
  let baseline = fallback;
  if (typeof document !== "undefined" && document.body) {
    const probe = document.createElement("div"), marker = document.createElement("span");
    probe.style.cssText = `all:initial;position:fixed;left:-100000px;top:0;visibility:hidden;pointer-events:none;white-space:pre;font:${font};line-height:${lineHeight}px;`;
    marker.style.cssText = "all:initial;display:inline-block;width:0;height:0;vertical-align:baseline;";
    probe.append("Mg", marker); document.body.append(probe);
    const rect = probe.getBoundingClientRect();
    if (rect.height > 0) baseline = marker.getBoundingClientRect().top - rect.top;
    probe.remove();
  }
  if (baselines.size >= 64) baselines.delete(baselines.keys().next().value!);
  baselines.set(key, baseline);
  return baseline;
}
/** The same local geometry drives rendering, editing, picking, and selection. */
export function layoutText(content: CreativeText, measure?: Measure, options?: CreativeTextLayout | null): TextLayout {
  const font = textFont(content), key = `${font}\0${JSON.stringify(options)}\0${content.text}`;
  if (!measure && layouts.has(key)) return layouts.get(key)!;
  const ctx = measure ? null : measureContext();
  if (ctx) ctx.font = font;
  const size = content.font_size, padding = size * .06, lineHeight = size * (options?.line_height ?? 1.3);
  const baseRead: Measure = measure ?? (ctx ? (line: string) => ctx.measureText(line) : (line: string) => ({ width: [...line].length * size * .6 }));
  let offset = 0;
  const read: Measure = (line, relative = 0) => {
    const raw = baseRead(line);
    let width = raw.width;
    if (ctx && options?.spans?.length) {
      width = 0;
      let run = "", runFont = "";
      for (const segment of textSegments(content.text, options, offset + relative, offset + relative + line.length)) {
        const nextFont = textFont({ ...content, font_weight: segment.style.font_weight ?? content.font_weight, font_style: segment.style.font_style ?? content.font_style });
        if (run && nextFont !== runFont) { ctx.font = runFont; width += ctx.measureText(run).width; run = ""; }
        runFont = nextFont; run += segment.text;
      }
      if (run) { ctx.font = runFont; width += ctx.measureText(run).width; }
      ctx.font = font;
    }
    return { ...raw, width: Math.max(0, width + [...line].length * (options?.letter_spacing ?? 0)) };
  };
  const hardLines = content.text.replace(/\r\n?/g, "\n").split("\n"), lines: string[] = [], starts: number[] = [], ys: number[] = [];
  let y = 0;
  for (const hard of hardLines) {
    const wrapped = options?.width ? wrapTextLine(hard, Math.max(1, options.width - padding * 2), read) : [hard];
    for (const line of wrapped) { starts.push(offset); lines.push(line); ys.push(y); offset += line.length; y += lineHeight; }
    offset++; y += options?.paragraph_spacing ?? 0;
  }
  const metrics = lines.map((line,i) => { offset = starts[i]; return read(line); });
  const left = Math.max(0, ...metrics.map(m => m.actualBoundingBoxLeft ?? 0));
  const sample = read("Mg"), ascent = sample.fontBoundingBoxAscent ?? size * .8, descent = sample.fontBoundingBoxDescent ?? size * .2;
  const baseline = measure ? (lineHeight - ascent - descent) / 2 + ascent : cssBaseline(font, lineHeight, (lineHeight - ascent - descent) / 2 + ascent);
  const layout = {
    lines, starts, ys, padding, lineHeight, align: options?.align ?? "left", insetX: padding + left, baseline: padding + baseline,
    width: options?.width ?? (Math.max(size * .3, ...metrics.map(m => Math.max(m.width, m.actualBoundingBoxRight ?? 0))) + left + padding * 2),
    height: Math.max(lineHeight, y - (options?.paragraph_spacing ?? 0)) + padding * 2,
  };
  // A bounded cache avoids measuring unchanged scene text on every pointer move.
  if (!measure && key.length <= 65536) {
    while (layouts.size && (layouts.size >= 128 || cachedCharacters + key.length > 65536)) {
      const oldest = layouts.keys().next().value!;
      layouts.delete(oldest); cachedCharacters -= oldest.length;
    }
    layouts.set(key, layout); cachedCharacters += key.length;
  }
  return layout;
}

/** Preserve explicit line breaks and whitespace; break long tokens like a CSS text box. */
function wrapTextLine(text: string, width: number, read: Measure): string[] {
  const result: string[] = [];
  let line = "", consumed = 0;
  for (const token of text.match(/\S+\s*|\s+/gu) ?? [""]) {
    if (line && read(line + token.trimEnd(), consumed).width > width) { result.push(line); consumed += line.length; line = ""; }
    if (read(token.trimEnd(), consumed + line.length).width <= width) { line += token; continue; }
    for (const char of token) {
      if (line && !/\s/u.test(char) && read(line + char, consumed).width > width) { result.push(line); consumed += line.length; line = ""; }
      line += char;
    }
  }
  result.push(line);
  return result;
}

const fontPresence = new Map<string, boolean>();
/** No font enumeration permission: compare fallback metrics and label unresolvable faces. */
export function canvasFontAvailable(family: string) {
  if (["sans-serif","serif","monospace","cursive"].includes(family)) return true;
  if (typeof document === "undefined") return true;
  if (fontPresence.has(family)) return fontPresence.get(family)!;
  const ctx = measureContext(); if (!ctx) return true;
  const sample = "MmwW0 123456789 il";
  const widths = ["monospace","serif"].map(fallback => { ctx.font = `32px ${fallback}`;const base=ctx.measureText(sample).width;ctx.font=`32px "${family}", ${fallback}`;return ctx.measureText(sample).width!==base; });
  const available = widths.some(Boolean); fontPresence.set(family,available);return available;
}
