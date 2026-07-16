/** M8c — vault note attachments in frontmatter (paths to external files). */

import { isSpreadsheetPath } from "$lib/utils/spreadsheetPreview";
import { serializeFrontmatter, stripFrontmatter } from "$lib/utils/vaultFrontmatter";

export interface VaultAttachment {
  path: string;
  label: string;
  mime?: string;
}

const MIME_BY_EXT: Record<string, string> = {
  pdf: "application/pdf",
  csv: "text/csv",
  tsv: "text/tab-separated-values",
  txt: "text/plain",
  md: "text/markdown",
  doc: "application/msword",
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  xls: "application/vnd.ms-excel",
  xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
};

export function guessMimeFromPath(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return MIME_BY_EXT[ext] ?? "application/octet-stream";
}

export function attachmentFileName(attachment: VaultAttachment): string {
  const fromPath = attachment.path.split("/").pop()?.split("\\").pop();
  return fromPath || attachment.label;
}

export function isPdfAttachment(attachment: VaultAttachment): boolean {
  if (attachment.mime?.includes("pdf")) return true;
  return attachment.path.toLowerCase().endsWith(".pdf");
}

export function isImageAttachment(attachment: VaultAttachment): boolean {
  const mime = attachment.mime ?? guessMimeFromPath(attachment.path);
  return mime.startsWith("image/");
}

export function isSpreadsheetAttachment(attachment: VaultAttachment): boolean {
  const mime = attachment.mime ?? guessMimeFromPath(attachment.path);
  if (mime.includes("spreadsheet") || mime.includes("csv") || mime.includes("tab-separated")) {
    return true;
  }
  return isSpreadsheetPath(attachment.path);
}

export function canPreviewAttachment(attachment: VaultAttachment): boolean {
  return (
    isPdfAttachment(attachment) ||
    isImageAttachment(attachment) ||
    isSpreadsheetAttachment(attachment)
  );
}

export function partitionAttachments(attachments: readonly VaultAttachment[]): {
  spreadsheet: VaultAttachment | null;
  others: VaultAttachment[];
} {
  const spreadsheet = attachments.find((row) => isSpreadsheetAttachment(row)) ?? null;
  const others = attachments.filter((row) => row !== spreadsheet);
  return { spreadsheet, others };
}

function yamlQuote(value: string): string {
  if (/^[a-zA-Z0-9_./\-]+$/.test(value)) return value;
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

function unyamlQuote(value: string): string {
  const trimmed = value.trim();
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1).replace(/\\"/g, '"');
  }
  return trimmed;
}

function parseAttachmentItemLine(rest: string): Partial<VaultAttachment> {
  const item: Partial<VaultAttachment> = {};
  const inline = rest.match(/^(path|label|mime):\s*(.+)$/);
  if (inline) {
    item[inline[1] as keyof VaultAttachment] = unyamlQuote(inline[2]!);
    return item;
  }
  return item;
}

export function parseAttachmentsFromFrontmatter(frontmatter: string): VaultAttachment[] {
  const lines = frontmatter.split("\n");
  const result: VaultAttachment[] = [];
  let inBlock = false;
  let current: Partial<VaultAttachment> | null = null;

  const flush = () => {
    if (current?.path?.trim()) {
      result.push({
        path: current.path.trim(),
        label: (current.label?.trim() || attachmentFileName({ path: current.path, label: "" })),
        mime: current.mime?.trim() || guessMimeFromPath(current.path),
      });
    }
    current = null;
  };

  for (const line of lines) {
    if (/^attachments:\s*(\[\])?\s*$/.test(line.trim())) {
      inBlock = true;
      continue;
    }
    if (!inBlock) continue;

    if (/^[^\s-]/.test(line)) {
      break;
    }

    const itemMatch = line.match(/^\s*-\s*(.*)$/);
    if (itemMatch) {
      flush();
      current = parseAttachmentItemLine(itemMatch[1]!.trim());
      continue;
    }

    const fieldMatch = line.match(/^\s+(path|label|mime):\s*(.+)$/);
    if (fieldMatch && current) {
      current[fieldMatch[1] as keyof VaultAttachment] = unyamlQuote(fieldMatch[2]!);
    }
  }

  flush();
  return result;
}

export function listAttachments(content: string): VaultAttachment[] {
  const { frontmatter } = stripFrontmatter(content);
  if (!frontmatter) return [];
  return parseAttachmentsFromFrontmatter(frontmatter);
}

function withoutAttachmentsBlock(lines: string[]): string[] {
  const out: string[] = [];
  let skipping = false;

  for (const line of lines) {
    if (/^attachments:\s*(\[\])?\s*$/.test(line.trim())) {
      skipping = true;
      continue;
    }
    if (skipping) {
      if (/^[^\s-]/.test(line)) {
        skipping = false;
        out.push(line);
      }
      continue;
    }
    out.push(line);
  }

  return out;
}

export function setAttachments(content: string, attachments: VaultAttachment[]): string {
  const { content: body, frontmatter } = stripFrontmatter(content);
  const baseLines = frontmatter ? withoutAttachmentsBlock(frontmatter.split("\n")) : [];

  if (attachments.length > 0) {
    baseLines.push("attachments:");
    for (const attachment of attachments) {
      baseLines.push(`  - path: ${yamlQuote(attachment.path)}`);
      baseLines.push(`    label: ${yamlQuote(attachment.label)}`);
      baseLines.push(`    mime: ${yamlQuote(attachment.mime ?? guessMimeFromPath(attachment.path))}`);
    }
  }

  if (baseLines.length === 0) {
    return body;
  }

  return serializeFrontmatter(baseLines.join("\n"), body);
}

export function addAttachments(
  content: string,
  incoming: VaultAttachment[],
): string {
  const existing = listAttachments(content);
  const merged = [...existing];
  for (const attachment of incoming) {
    if (merged.some((row) => row.path === attachment.path)) continue;
    merged.push({
      ...attachment,
      mime: attachment.mime ?? guessMimeFromPath(attachment.path),
    });
  }
  return setAttachments(content, merged);
}

export function removeAttachment(content: string, path: string): string {
  const next = listAttachments(content).filter((row) => row.path !== path);
  return setAttachments(content, next);
}

export function mergeAttachmentLists(
  existing: VaultAttachment[],
  incoming: VaultAttachment[],
): VaultAttachment[] {
  const merged = [...existing];
  for (const attachment of incoming) {
    if (merged.some((row) => row.path === attachment.path)) continue;
    merged.push(attachment);
  }
  return merged;
}
