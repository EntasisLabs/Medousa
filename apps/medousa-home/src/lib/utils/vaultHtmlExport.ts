/**
 * Vault note → self-contained HTML via shared export prep.
 * Reuses the hydrated print-paper mount (Liquid frozen, chrome stripped) and
 * inlines the export stylesheet so the file stands alone.
 */

import {
  saveExportBlob,
  vaultExportFilename,
  type VaultExportOptions,
} from "./vaultExportOptions";
import {
  prepareVaultExportMount,
  sanitizeUnsupportedCssColors,
} from "./vaultExportPrep";
import { renderMarkdownPreview } from "$lib/markdown";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { noteHasSlidesDeck } from "$lib/utils/markdownSlides";
import { prepareSlidesExportMarkdown } from "./vaultExportPrep";

export function vaultHtmlFilename(title: string): string {
  return vaultExportFilename(title, "html");
}

export function vaultMarkdownFilename(title: string): string {
  return vaultExportFilename(title, "markdown");
}

export async function saveVaultNoteHtmlBlob(
  blob: Blob,
  filename: string,
): Promise<boolean> {
  return saveExportBlob(blob, filename, "html");
}

export async function saveVaultNoteMarkdownBlob(
  blob: Blob,
  filename: string,
): Promise<boolean> {
  return saveExportBlob(blob, filename, "markdown");
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Hydrate note markdown → self-contained HTML blob. */
export async function renderVaultNoteHtmlBlob(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
  notePath?: string | null;
  exportOptions?: Partial<VaultExportOptions> | null;
}): Promise<Blob> {
  const prepared = await prepareVaultExportMount({
    title: options.title,
    content: options.content,
    labelByPath: options.labelByPath,
    notePath: options.notePath,
    options: options.exportOptions,
  });
  try {
    sanitizeUnsupportedCssColors(prepared.mount);
    const styleText = prepared.mount.querySelector("style")?.textContent ?? "";
    const title = escapeHtml(options.title);
    const inner = prepared.mount.innerHTML;
    const doc = [
      "<!doctype html>",
      '<html lang="en">',
      "<head>",
      '<meta charset="utf-8" />',
      '<meta name="viewport" content="width=device-width, initial-scale=1" />',
      `<title>${title}</title>`,
      "<style>",
      "body{margin:0;background:#f6f5f2;font-family:system-ui,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;}",
      ".vault-pdf-export-shell{position:static;left:auto;top:auto;width:auto;visibility:visible;pointer-events:auto;z-index:auto;display:flex;justify-content:center;}",
      styleText,
      "</style>",
      "</head>",
      "<body>",
      `<div class="vault-pdf-export-shell">${inner}</div>`,
      "</body>",
      "</html>",
    ].join("\n");
    return new Blob([doc], { type: "text/html" });
  } finally {
    prepared.dispose();
  }
}

/** Clean markdown export — slides wrapped, frontmatter preserved. */
export function renderVaultNoteMarkdownBlob(options: {
  content: string;
}): Blob {
  let content = options.content;
  if (noteHasSlidesDeck(content)) {
    content = prepareSlidesExportMarkdown(content);
  }
  // Normalize trailing newline; keep fences + frontmatter intact.
  const text = content.endsWith("\n") ? content : `${content}\n`;
  return new Blob([text], { type: "text/markdown" });
}

/** Flattened markdown — no interactive fences, prose + tables only. */
export function renderVaultNoteMarkdownFlattenedBlob(options: {
  content: string;
  labelByPath: Map<string, string>;
}): Blob {
  const { content: body } = stripFrontmatter(options.content);
  const html = renderMarkdownPreview(body, {
    titleByPath: options.labelByPath,
  });
  // Strip tags for a plain-text-leaning flattened note.
  const text = html
    .replace(/<pre[\s\S]*?<\/pre>/g, (m) =>
      m.replace(/<[^>]+>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&"),
    )
    .replace(/<br\s*\/?>/g, "\n")
    .replace(/<\/(p|h[1-6]|li|tr|table|ul|ol|blockquote)>/g, "\n")
    .replace(/<li[^>]*>/g, "- ")
    .replace(/<[^>]+>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  return new Blob([`${text}\n`], { type: "text/markdown" });
}
