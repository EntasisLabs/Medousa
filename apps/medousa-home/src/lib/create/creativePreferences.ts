import type { DrawBrushKind } from "$lib/draw/drawDocument";
import { createDrawBrush, DRAW_BRUSHES } from "$lib/draw/drawDocument";
import type { DrawEraserMode } from "$lib/draw/drawInteraction";
import type { CreativeTool } from "./creativeScene";
import type { CanvasFill, CanvasShape, CanvasSize } from "./creativeInteraction";
import { canvasFonts, defaultTextStyle, type TextStylePatch } from "./creativeText";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";

export type CreativePreferences = {
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
};
const key = (scope: string) => workshopScopedStorageKey("medousa:create-tools:v1", scope);
export function loadCreativePreferences(scope: string): CreativePreferences {
  const defaults: CreativePreferences = {
    tool: "select", selectionMode: "box", brushKind: "pen",
    brushSizes: Object.fromEntries(DRAW_BRUSHES.map(b => [b.kind, createDrawBrush(b.kind).size])) as Record<DrawBrushKind, number>,
    eraserSize: 12, eraserMode: "partial", fingerDraw: false,
    color: null, size: "m", shape: "rectangle", fill: "soft", textStyle: { ...defaultTextStyle },
  };
  try {
    const saved = JSON.parse(localStorage.getItem(key(scope)) ?? "null");
    if (!saved || typeof saved !== "object")
      return defaults;
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
    if (saved.textStyle && typeof saved.textStyle === "object") {
      const text = saved.textStyle;
      if (canvasFonts.some(f => f.value === text.font_family)) defaults.textStyle.font_family = text.font_family;
      if (typeof text.font_size === "number" && Number.isFinite(text.font_size) && text.font_size >= 4 && text.font_size <= 512) defaults.textStyle.font_size = text.font_size;
      if (text.font_weight === 400 || text.font_weight === 700) defaults.textStyle.font_weight = text.font_weight;
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
