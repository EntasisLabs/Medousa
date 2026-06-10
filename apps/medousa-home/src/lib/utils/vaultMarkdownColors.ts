/** Text color tokens for vault markdown (portable syntax + render classes). */

export type MarkdownColorId =
  | "red"
  | "orange"
  | "yellow"
  | "green"
  | "blue"
  | "purple"
  | "pink";

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

export function markdownColorClass(color: MarkdownColorId): string {
  return `markdown-color markdown-color-${color}`;
}

/** Toolbar insert: `{{red|text}}` — preprocessed to colored span at render time. */
export function markdownColorOpenTag(color: MarkdownColorId): string {
  return `{{${color}|`;
}

export const MARKDOWN_COLOR_CLOSE_TAG = "}}";

export function colorSpanHtml(color: MarkdownColorId, text: string): string {
  const hex = MARKDOWN_COLOR_HEX[color];
  return `<span class="${markdownColorClass(color)}" style="color: ${hex}">${text}</span>`;
}

export function isMarkdownColorId(value: string): value is MarkdownColorId {
  return (MARKDOWN_COLOR_IDS as string[]).includes(value);
}
