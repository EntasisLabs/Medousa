import type { LineStyle } from "./creativeLine";
import type { DrawBrushKind } from "$lib/draw/drawDocument";
import { createDrawBrush, DRAW_BRUSHES } from "$lib/draw/drawDocument";
import type { DrawEraserMode } from "$lib/draw/drawInteraction";
import type { CreativeTool } from "./creativeScene";
import type { CanvasFill, CanvasShape, CanvasSize } from "./creativeInteraction";
import { canvasFonts, defaultTextStyle, type TextStylePatch } from "./creativeText";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";
import { defaultLayout, type LayoutPreferences } from "./creativeLayout";

export type CreativePreferences = {
  layout: LayoutPreferences;
  tool: CreativeTool;
  selectionMode: "box" | "lasso";
  brushKind: DrawBrushKind;
  brushSizes: Record<DrawBrushKind, number>;
  eraserSize: number;
  eraserMode: DrawEraserMode;
  fingerDraw: boolean;
  color: string | null;
  size: CanvasSize;
  shape: CanvasShape;
  fill: CanvasFill;
  textStyle: TextStylePatch;
  lineStyles: { line: LineStyle; arrow: LineStyle };
  detailsSections: { properties: boolean; layers: boolean };
};
const key = (scope: string) => workshopScopedStorageKey("medousa:create-tools:v1", scope);
export function loadCreativePreferences(scope: string): CreativePreferences {
  const defaults: CreativePreferences = {
    layout: { ...defaultLayout },
    tool: "select", selectionMode: "box", brushKind: "pen",
    brushSizes: Object.fromEntries(DRAW_BRUSHES.map(b => [b.kind, createDrawBrush(b.kind).size])) as Record<DrawBrushKind, number>,
    eraserSize: 12, eraserMode: "partial", fingerDraw: false,
    color: null, size: "m", shape: "rectangle", fill: "soft", textStyle: { ...defaultTextStyle },
    lineStyles: { line: { stroke_width: 4, dashed: false, start_arrow: false, end_arrow: false }, arrow: { stroke_width: 4, dashed: false, start_arrow: false, end_arrow: true } },
    detailsSections: { properties: true, layers: true },
  };
  try {
    const saved = JSON.parse(localStorage.getItem(key(scope)) ?? "null");
    if (!saved || typeof saved !== "object")
      return defaults;
    if (["none", "dots", "lines"].includes(saved.layout?.grid)) defaults.layout.grid = saved.layout.grid;
    if ([8, 16, 32, 64].includes(saved.layout?.gridSize)) defaults.layout.gridSize = saved.layout.gridSize;
    for (const key of ["snapGrid", "smartGuides"] as const) if (typeof saved.layout?.[key] === "boolean") defaults.layout[key] = saved.layout[key];
    for (const section of ["properties", "layers"] as const) {
      if (typeof saved.detailsSections?.[section] === "boolean") defaults.detailsSections[section] = saved.detailsSections[section];
    }
    const choices = {
      tool: ["select", "hand", "ink", "erase", "shape", "text", "frame"],
      selectionMode: ["box", "lasso"], brushKind: DRAW_BRUSHES.map(b => b.kind),
      eraserMode: ["partial", "stroke"], size: ["s", "m", "l", "xl"],
      shape: ["rectangle", "ellipse", "line", "arrow"], fill: ["none", "soft", "solid"],
    };
    for (const [name, values] of Object.entries(choices)) {
      if (values.includes(saved[name]))
        Object.assign(defaults, { [name]: saved[name] });
    }
    const validSize = (n: unknown): n is number => typeof n === "number" && Number.isFinite(n) && n >= 1 && n <= 160;
    for (const b of DRAW_BRUSHES) {
      if (validSize(saved.brushSizes?.[b.kind]))
        defaults.brushSizes[b.kind] = saved.brushSizes[b.kind];
    }
    for (const kind of ["line", "arrow"] as const) {
      const line = saved.lineStyles?.[kind];
      if (!line || typeof line !== "object") continue;
      if (typeof line.stroke_width === "number" && Number.isFinite(line.stroke_width) && line.stroke_width >= .5 && line.stroke_width <= 100) defaults.lineStyles[kind].stroke_width = line.stroke_width;
      for (const key of ["dashed", "start_arrow", "end_arrow"] as const) if (typeof line[key] === "boolean") defaults.lineStyles[kind][key] = line[key];
    }
    if (saved.textStyle && typeof saved.textStyle === "object") {
      const text = saved.textStyle;
      if (text.text_width === null || typeof text.text_width === "number" && Number.isFinite(text.text_width) && text.text_width >= 4 && text.text_width <= 1e7) defaults.textStyle.text_width = text.text_width;
      if (["left", "center", "right"].includes(text.text_align)) defaults.textStyle.text_align = text.text_align;
      if (typeof text.line_height === "number" && Number.isFinite(text.line_height) && text.line_height >= .8 && text.line_height <= 4) defaults.textStyle.line_height = text.line_height;
      if (typeof text.font_family === "string" && /^[\p{L}\p{N} _-]{1,128}$/u.test(text.font_family)) defaults.textStyle.font_family = text.font_family;
      if (typeof text.font_size === "number" && Number.isFinite(text.font_size) && text.font_size >= 4 && text.font_size <= 512) defaults.textStyle.font_size = text.font_size;
      if (Number.isInteger(text.font_weight) && text.font_weight >= 100 && text.font_weight <= 900) defaults.textStyle.font_weight = text.font_weight;
      for (const [key,min,max] of [["letter_spacing",-100,100],["paragraph_spacing",0,512]] as const) if (typeof text[key] === "number" && Number.isFinite(text[key]) && text[key] >= min && text[key] <= max) defaults.textStyle[key] = text[key];
      if (typeof text.underline === "boolean") defaults.textStyle.underline = text.underline;
      if (text.font_style === "normal" || text.font_style === "italic") defaults.textStyle.font_style = text.font_style;
    }
    if (validSize(saved.eraserSize))
      defaults.eraserSize = saved.eraserSize;
    if (typeof saved.fingerDraw === "boolean")
      defaults.fingerDraw = saved.fingerDraw;
    if (typeof saved.color === "string" && /^#[\da-f]{6}$/i.test(saved.color))
      defaults.color = saved.color;
  } catch { /* Unavailable or old local preferences never block a canvas. */ }
  return defaults;
}
export function saveCreativePreferences(scope: string, preferences: CreativePreferences) {
  try {
    localStorage.setItem(key(scope), JSON.stringify(preferences));
  } catch { /* Private browsing/storage limits leave this view's tools usable. */ }
}
