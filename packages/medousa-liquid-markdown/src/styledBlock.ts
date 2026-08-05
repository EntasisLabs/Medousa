/**
 * Shared styled-block (` ```block ` / turbo-fish) parse + CSS resolution.
 */

export type BlockFont = "sans" | "serif" | "mono";
export type BlockAlign = "left" | "center" | "right" | "justify";
export type BlockSizeToken = "sm" | "md" | "lg" | "xl";
export type BlockSpacingToken = "tight" | "normal" | "relaxed";

export type LiquidBlockProps = {
  body: string;
  id?: string;
  font?: BlockFont;
  size?: string;
  align?: BlockAlign;
  spacing?: string;
};

const FONTS = new Set<BlockFont>(["sans", "serif", "mono"]);
const ALIGNS = new Set<BlockAlign>(["left", "center", "right", "justify"]);
const SIZE_PX: Record<BlockSizeToken, string> = {
  sm: "0.875rem",
  md: "1rem",
  lg: "1.125rem",
  xl: "1.35rem",
};
const SPACING: Record<BlockSpacingToken, string> = {
  tight: "1.25",
  normal: "1.55",
  relaxed: "1.8",
};

const FONT_STACK: Record<BlockFont, string> = {
  sans: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
  serif: 'ui-serif, Georgia, Cambria, "Times New Roman", Times, serif',
  mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
};

export function parseKvLines(header: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of header.replace(/\r\n/g, "\n").split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (key && value) out[key] = value;
  }
  return out;
}

/** Split fence/turbo body into KV header + prose after `---`. */
export function splitMetaBody(source: string): { header: string; body: string } {
  const normalized = source.replace(/\r\n/g, "\n");
  const sep = normalized.search(/^---[ \t]*$/m);
  if (sep < 0) {
    return { header: normalized, body: "" };
  }
  return {
    header: normalized.slice(0, sep),
    body: normalized.slice(sep).replace(/^---[ \t]*\n?/, "").replace(/\s+$/, ""),
  };
}

export function parseStyledBlockFields(fields: Record<string, string>, body: string): LiquidBlockProps {
  const props: LiquidBlockProps = { body: body.trim() };
  if (fields.id?.trim()) props.id = fields.id.trim();
  const font = fields.font?.trim().toLowerCase();
  if (font && FONTS.has(font as BlockFont)) props.font = font as BlockFont;
  if (fields.size?.trim()) props.size = fields.size.trim();
  const align = fields.align?.trim().toLowerCase();
  if (align && ALIGNS.has(align as BlockAlign)) props.align = align as BlockAlign;
  if (fields.spacing?.trim()) props.spacing = fields.spacing.trim();
  return props;
}

export function parseStyledBlockBody(source: string): LiquidBlockProps | null {
  const { header, body } = splitMetaBody(source);
  const fields = parseKvLines(header);
  const prose = body || fields.body?.trim() || "";
  if (!prose && !fields.id && !fields.font && !fields.size && !fields.align && !fields.spacing) {
    return null;
  }
  return parseStyledBlockFields(fields, prose || " ");
}

export function serializeStyledBlockFence(props: LiquidBlockProps): string {
  const lines = ["```block"];
  if (props.id?.trim()) lines.push(`id: ${props.id.trim()}`);
  if (props.font) lines.push(`font: ${props.font}`);
  if (props.size?.trim()) lines.push(`size: ${props.size.trim()}`);
  if (props.align) lines.push(`align: ${props.align}`);
  if (props.spacing?.trim()) lines.push(`spacing: ${props.spacing.trim()}`);
  lines.push("---");
  lines.push(props.body.replace(/\s+$/, "") || " ");
  lines.push("```", "");
  return lines.join("\n");
}

export function resolveBlockFontFamily(font?: string): string | undefined {
  const f = font?.trim().toLowerCase();
  if (f && FONTS.has(f as BlockFont)) return FONT_STACK[f as BlockFont];
  return undefined;
}

export function resolveBlockFontSize(size?: string): string | undefined {
  if (!size?.trim()) return undefined;
  const t = size.trim().toLowerCase();
  if (t in SIZE_PX) return SIZE_PX[t as BlockSizeToken];
  if (/^\d+(\.\d+)?$/.test(t)) return `${t}px`;
  if (/^\d+(\.\d+)?(px|rem|em)$/i.test(t)) return t;
  return undefined;
}

export function resolveBlockLineHeight(spacing?: string): string | undefined {
  if (!spacing?.trim()) return undefined;
  const t = spacing.trim().toLowerCase();
  if (t in SPACING) return SPACING[t as BlockSpacingToken];
  const n = Number(t);
  if (Number.isFinite(n) && n >= 1 && n <= 3) return String(n);
  return undefined;
}

/** CSS custom properties for `.liquid-styled-block`. */
export function styledBlockCssVars(props: LiquidBlockProps): Record<string, string> {
  const vars: Record<string, string> = {};
  const font = resolveBlockFontFamily(props.font);
  const size = resolveBlockFontSize(props.size);
  const spacing = resolveBlockLineHeight(props.spacing);
  if (font) vars["--block-font"] = font;
  if (size) vars["--block-size"] = size;
  if (props.align) vars["--block-align"] = props.align;
  if (spacing) vars["--block-spacing"] = spacing;
  return vars;
}

export function isBlockFont(value: string): value is BlockFont {
  return FONTS.has(value.trim().toLowerCase() as BlockFont);
}

export function isBlockSizeToken(value: string): boolean {
  const t = value.trim().toLowerCase();
  return t in SIZE_PX || /^\d+(\.\d+)?$/.test(t) || /^\d+(\.\d+)?(px|rem|em)$/i.test(t);
}
