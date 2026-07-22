/**
 * Selection font family / size marks — `{{font:serif|text}}` / `{{size:lg|text}}`.
 */

import {
  resolveBlockFontFamily,
  resolveBlockFontSize,
  type BlockFont,
} from "$lib/markdown/styledBlock";

export type MarkdownFontFamily = BlockFont;
export type MarkdownFontSizeToken = "sm" | "md" | "lg" | "xl";

const FONTS = new Set<MarkdownFontFamily>(["sans", "serif", "mono"]);
const SIZE_TOKENS = new Set<MarkdownFontSizeToken>(["sm", "md", "lg", "xl"]);

export const MARKDOWN_FONT_FAMILY_OPTIONS: {
  id: MarkdownFontFamily;
  label: string;
}[] = [
  { id: "sans", label: "Sans" },
  { id: "serif", label: "Serif" },
  { id: "mono", label: "Mono" },
];

export const MARKDOWN_FONT_SIZE_OPTIONS: {
  id: MarkdownFontSizeToken;
  label: string;
}[] = [
  { id: "sm", label: "S" },
  { id: "md", label: "M" },
  { id: "lg", label: "L" },
  { id: "xl", label: "XL" },
];

export function isMarkdownFontFamily(value: string): value is MarkdownFontFamily {
  return FONTS.has(value.trim().toLowerCase() as MarkdownFontFamily);
}

export function isMarkdownFontSizeToken(value: string): boolean {
  const t = value.trim().toLowerCase();
  if (SIZE_TOKENS.has(t as MarkdownFontSizeToken)) return true;
  if (/^\d+(\.\d+)?$/.test(t)) return true;
  if (/^\d+(\.\d+)?(px|rem|em)$/i.test(t)) return true;
  return false;
}

export function normalizeMarkdownFontFamily(
  value: string,
): MarkdownFontFamily | null {
  const t = value.trim().toLowerCase();
  return isMarkdownFontFamily(t) ? (t as MarkdownFontFamily) : null;
}

export function normalizeMarkdownFontSize(value: string): string | null {
  const t = value.trim();
  if (!isMarkdownFontSizeToken(t)) return null;
  return t.toLowerCase().replace(/px$/i, (m) => m.toLowerCase());
}

export function resolveMarkdownFontFamilyCss(
  value: string,
): string | undefined {
  return resolveBlockFontFamily(value);
}

export function resolveMarkdownFontSizeCss(value: string): string | undefined {
  return resolveBlockFontSize(value);
}

export function fontFamilySpanHtml(font: string, innerHtml: string): string {
  const css = resolveMarkdownFontFamilyCss(font);
  const safe = font.trim().toLowerCase();
  const style = css ? ` style="font-family: ${css}"` : "";
  return `<span class="markdown-font-family" data-md-font="${safe}"${style}>${innerHtml}</span>`;
}

export function fontSizeSpanHtml(size: string, innerHtml: string): string {
  const css = resolveMarkdownFontSizeCss(size);
  const safe = size.trim().toLowerCase();
  const style = css ? ` style="font-size: ${css}"` : "";
  return `<span class="markdown-font-size" data-md-size="${safe}"${style}>${innerHtml}</span>`;
}
