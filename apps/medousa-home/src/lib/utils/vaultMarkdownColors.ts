/** Text color tokens for vault markdown (portable syntax + render classes). */

export type MarkdownColorId =
  | "red"
  | "orange"
  | "yellow"
  | "green"
  | "blue"
  | "purple"
  | "pink";

/** Named swatch or validated `#RGB` / `#RRGGBB` / `#RRGGBBAA`. */
export type MarkdownColorToken = MarkdownColorId | string;

export interface MarkdownColorOption {
  id: MarkdownColorId;
  label: string;
  swatch: string;
}

export const MARKDOWN_COLOR_HEX: Record<MarkdownColorId, string> = {
  red: "#f87171",
  orange: "#fb923c",
  yellow: "#facc15",
  green: "#4ade80",
  blue: "#60a5fa",
  purple: "#c084fc",
  pink: "#f472b6",
};

export const MARKDOWN_COLOR_HEX_LIGHT: Record<MarkdownColorId, string> = {
  red: "#dc2626",
  orange: "#ea580c",
  yellow: "#ca8a04",
  green: "#16a34a",
  blue: "#2563eb",
  purple: "#9333ea",
  pink: "#db2777",
};

export const MARKDOWN_COLOR_OPTIONS: MarkdownColorOption[] = [
  { id: "red", label: "Red", swatch: MARKDOWN_COLOR_HEX.red },
  { id: "orange", label: "Orange", swatch: MARKDOWN_COLOR_HEX.orange },
  { id: "yellow", label: "Yellow", swatch: MARKDOWN_COLOR_HEX.yellow },
  { id: "green", label: "Green", swatch: MARKDOWN_COLOR_HEX.green },
  { id: "blue", label: "Blue", swatch: MARKDOWN_COLOR_HEX.blue },
  { id: "purple", label: "Purple", swatch: MARKDOWN_COLOR_HEX.purple },
  { id: "pink", label: "Pink", swatch: MARKDOWN_COLOR_HEX.pink },
];

export const MARKDOWN_COLOR_IDS = MARKDOWN_COLOR_OPTIONS.map((option) => option.id);

const HEX_COLOR_RE = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;

export function isMarkdownColorId(value: string): value is MarkdownColorId {
  return (MARKDOWN_COLOR_IDS as string[]).includes(value.toLowerCase());
}

/** Accepts `#RGB`, `#RRGGBB`, `#RRGGBBAA` (with leading #). */
export function isMarkdownHexColor(value: string): boolean {
  return HEX_COLOR_RE.test(value.trim());
}

/** Normalize a hex token for storage (`#RRGGBB` uppercase). Returns null if invalid. */
export function normalizeMarkdownHexColor(value: string): string | null {
  const trimmed = value.trim();
  if (!isMarkdownHexColor(trimmed)) return null;
  const raw = trimmed.slice(1);
  if (raw.length === 3) {
    const expanded = raw
      .split("")
      .map((ch) => `${ch}${ch}`)
      .join("");
    return `#${expanded.toUpperCase()}`;
  }
  return `#${raw.toUpperCase()}`;
}

export function isMarkdownColorToken(value: string): boolean {
  return isMarkdownColorId(value) || isMarkdownHexColor(value);
}

/** Resolve any accepted token to a CSS color, or null. */
export function resolveMarkdownColorCss(token: string): string | null {
  const trimmed = token.trim();
  if (isMarkdownColorId(trimmed)) {
    return MARKDOWN_COLOR_HEX[trimmed.toLowerCase() as MarkdownColorId];
  }
  return normalizeMarkdownHexColor(trimmed);
}

export function markdownColorClass(color: MarkdownColorId): string {
  return `markdown-color markdown-color-${color}`;
}

/** Toolbar insert: `{{red|text}}` or `{{#FF5500|text}}`. */
export function markdownColorOpenTag(color: MarkdownColorToken): string {
  if (isMarkdownColorId(color)) return `{{${color.toLowerCase()}|`;
  const hex = normalizeMarkdownHexColor(color);
  return hex ? `{{${hex}|` : `{{`;
}

export const MARKDOWN_COLOR_CLOSE_TAG = "}}";

export function colorSpanHtml(color: MarkdownColorToken, text: string): string {
  if (isMarkdownColorId(color)) {
    const id = color.toLowerCase() as MarkdownColorId;
    const hex = MARKDOWN_COLOR_HEX[id];
    return `<span class="${markdownColorClass(id)}" style="color: ${hex}">${text}</span>`;
  }
  const hex = normalizeMarkdownHexColor(color);
  if (!hex) return text;
  return `<span class="markdown-color markdown-color-hex" style="color: ${hex}">${text}</span>`;
}

/** Serialize color markup for the note source. */
export function colorMarkupToken(color: MarkdownColorToken, inner: string): string {
  if (isMarkdownColorId(color)) {
    return `{{${color.toLowerCase()}|${inner}}}`;
  }
  const hex = normalizeMarkdownHexColor(color);
  if (!hex) return inner;
  return `{{${hex}|${inner}}}`;
}
