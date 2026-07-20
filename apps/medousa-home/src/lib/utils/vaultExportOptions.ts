/**
 * Shared vault export settings (PDF + Word).
 * Persisted in localStorage — last-used preferences.
 */

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

const EXPORT_OPTIONS_KEY = "medousa-vault-export-options";

export type VaultExportFormat = "pdf" | "docx";
export type VaultExportFontFamily = "system" | "serif" | "mono";
export type VaultExportPageSize = "letter" | "a4";
export type VaultExportOrientation = "portrait" | "landscape";
export type VaultExportMargins = "comfortable" | "compact" | "wide";

export interface VaultExportOptions {
  fontFamily: VaultExportFontFamily;
  /** Body font size in px (11–16). */
  baseFontPx: number;
  pageSize: VaultExportPageSize;
  orientation: VaultExportOrientation;
  margins: VaultExportMargins;
  /** Start a new page before each H2. */
  breakBeforeH2: boolean;
  /** Prefer keeping tables/code/embeds together across page breaks. */
  keepTogether: boolean;
  /** Include frontmatter author in the export byline. */
  includeAuthor: boolean;
  /** Include frontmatter date in the export byline. */
  includeDate: boolean;
}

export const DEFAULT_VAULT_EXPORT_OPTIONS: VaultExportOptions = {
  fontFamily: "system",
  baseFontPx: 14,
  pageSize: "letter",
  orientation: "portrait",
  margins: "comfortable",
  breakBeforeH2: false,
  keepTogether: false,
  includeAuthor: false,
  includeDate: false,
};

const FONT_STACKS: Record<VaultExportFontFamily, string> = {
  system:
    'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  serif: 'Georgia, "Times New Roman", Times, serif',
  mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
};

const MONO_STACK =
  'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace';

/** Inch margins [top, right, bottom, left] for html2pdf / jsPDF. */
const MARGIN_INCHES: Record<VaultExportMargins, [number, number, number, number]> = {
  comfortable: [0.55, 0.6, 0.55, 0.6],
  compact: [0.4, 0.45, 0.4, 0.45],
  wide: [0.75, 0.9, 0.75, 0.9],
};

/** Twips (1/20 pt) page margins for docx section properties. */
const MARGIN_TWIPS: Record<VaultExportMargins, { top: number; right: number; bottom: number; left: number }> = {
  comfortable: { top: 792, right: 864, bottom: 792, left: 864 },
  compact: { top: 576, right: 648, bottom: 576, left: 648 },
  wide: { top: 1080, right: 1296, bottom: 1080, left: 1296 },
};

export function exportFontStack(family: VaultExportFontFamily): string {
  return FONT_STACKS[family] ?? FONT_STACKS.system;
}

export function exportMonoFontStack(): string {
  return MONO_STACK;
}

/** Docx / Word font name for body text. */
export function exportDocxFontName(family: VaultExportFontFamily): string {
  if (family === "serif") return "Georgia";
  if (family === "mono") return "Courier New";
  return "Calibri";
}

export function exportMarginInches(
  margins: VaultExportMargins,
): [number, number, number, number] {
  return MARGIN_INCHES[margins] ?? MARGIN_INCHES.comfortable;
}

export function exportMarginTwips(margins: VaultExportMargins) {
  return MARGIN_TWIPS[margins] ?? MARGIN_TWIPS.comfortable;
}

/** Usable content width in DXA/twips for Word tables (page − left/right margins). */
export function exportDocxContentWidthDxa(options: VaultExportOptions): number {
  const page =
    options.pageSize === "a4"
      ? { width: 11906, height: 16838 }
      : { width: 12240, height: 15840 };
  const margins = exportMarginTwips(options.margins);
  const pageW =
    options.orientation === "landscape" ? page.height : page.width;
  return Math.max(2400, pageW - margins.left - margins.right);
}

/** Content width in px for the off-screen export mount (approx letter/A4 minus margins). */
export function exportContentWidthPx(options: VaultExportOptions): number {
  const pageWidthIn = options.pageSize === "a4" ? 8.27 : 8.5;
  const [, right, , left] = exportMarginInches(options.margins);
  const usable = pageWidthIn - left - right;
  const landscape =
    options.orientation === "landscape"
      ? (options.pageSize === "a4" ? 11.69 : 11) - left - right
      : usable;
  return Math.round(Math.max(480, landscape * 96));
}

export function clampBaseFontPx(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_VAULT_EXPORT_OPTIONS.baseFontPx;
  return Math.min(16, Math.max(11, Math.round(value)));
}

export function normalizeVaultExportOptions(
  partial?: Partial<VaultExportOptions> | null,
): VaultExportOptions {
  const base = { ...DEFAULT_VAULT_EXPORT_OPTIONS, ...partial };
  const fontFamily: VaultExportFontFamily =
    base.fontFamily === "serif" || base.fontFamily === "mono"
      ? base.fontFamily
      : "system";
  const pageSize: VaultExportPageSize =
    base.pageSize === "a4" ? "a4" : "letter";
  const orientation: VaultExportOrientation =
    base.orientation === "landscape" ? "landscape" : "portrait";
  const margins: VaultExportMargins =
    base.margins === "compact" || base.margins === "wide"
      ? base.margins
      : "comfortable";
  return {
    fontFamily,
    baseFontPx: clampBaseFontPx(base.baseFontPx),
    pageSize,
    orientation,
    margins,
    breakBeforeH2: Boolean(base.breakBeforeH2),
    keepTogether: Boolean(base.keepTogether),
    includeAuthor: Boolean(base.includeAuthor),
    includeDate: Boolean(base.includeDate),
  };
}

export function readVaultExportOptions(): VaultExportOptions {
  if (typeof localStorage === "undefined") return { ...DEFAULT_VAULT_EXPORT_OPTIONS };
  try {
    const raw = localStorage.getItem(EXPORT_OPTIONS_KEY);
    if (!raw) return { ...DEFAULT_VAULT_EXPORT_OPTIONS };
    return normalizeVaultExportOptions(JSON.parse(raw) as Partial<VaultExportOptions>);
  } catch {
    return { ...DEFAULT_VAULT_EXPORT_OPTIONS };
  }
}

export function writeVaultExportOptions(options: VaultExportOptions): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(
    EXPORT_OPTIONS_KEY,
    JSON.stringify(normalizeVaultExportOptions(options)),
  );
}

export function slugifyExportFilename(title: string): string {
  const slug = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "note";
}

export function vaultExportFilename(
  title: string,
  format: VaultExportFormat,
): string {
  const ext = format === "docx" ? "docx" : "pdf";
  return `${slugifyExportFilename(title)}.${ext}`;
}

/** Persist a rendered blob (Tauri save dialog or browser download). Returns false if cancelled. */
export async function saveExportBlob(
  blob: Blob,
  filename: string,
  format: VaultExportFormat,
): Promise<boolean> {
  if (isTauri()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({
      defaultPath: filename,
      filters:
        format === "docx"
          ? [{ name: "Word", extensions: ["docx"] }]
          : [{ name: "PDF", extensions: ["pdf"] }],
      title: format === "docx" ? "Export note as Word" : "Export note as PDF",
    });
    if (!path) return false;
    const bytes = new Uint8Array(await blob.arrayBuffer());
    await invoke("write_file_bytes", { path, bytes: Array.from(bytes) });
    return true;
  }

  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
  return true;
}
