/** Phase G3e — render `![[note]]` embeds in vault preview (source file unchanged). */

import { getVaultNote } from "$lib/daemon";
import { renderMarkdown } from "$lib/markdown/render";
import type { VaultNote } from "$lib/types/vault";
import { vaultDisplayTitle } from "$lib/utils/formatVault";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { parseWikilinkTarget, resolveWikilinkTarget } from "$lib/utils/resolveWikilink";
import { escapeAttr, escapeHtml } from "$lib/markdown/escape";

const TRANSCLUSION_RE = /!\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|([^\]]+))?\]\]/g;

const CONTENT_CACHE_TTL_MS = 30_000;
const contentCache = new Map<string, { content: string; fetchedAt: number }>();

export function invalidateTransclusionCache(path?: string) {
  if (path) {
    contentCache.delete(path);
    return;
  }
  contentCache.clear();
}

export function hasTransclusionBlocks(source: string): boolean {
  return /!\[\[[^\]|#]+(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]/.test(source);
}

export interface TransclusionBlock {
  fullMatch: string;
  target: string;
  heading: string | null;
}

export function extractTransclusionBlocks(source: string): TransclusionBlock[] {
  const blocks: TransclusionBlock[] = [];
  for (const match of source.matchAll(TRANSCLUSION_RE)) {
    blocks.push({
      fullMatch: match[0],
      target: match[1]!.trim(),
      heading: match[2]?.trim() || null,
    });
  }
  return blocks;
}

async function loadNoteContent(
  path: string,
  selectedPath: string | null,
  selectedContent: string,
): Promise<string | null> {
  if (path === selectedPath) {
    return selectedContent;
  }

  const cached = contentCache.get(path);
  if (cached && Date.now() - cached.fetchedAt < CONTENT_CACHE_TTL_MS) {
    return cached.content;
  }

  try {
    const response = await getVaultNote(path);
    contentCache.set(path, { content: response.content, fetchedAt: Date.now() });
    return response.content;
  } catch {
    return null;
  }
}

function renderTransclusionError(message: string): string {
  return `<div class="markdown-transclusion markdown-transclusion-error" role="note">${escapeHtml(message)}</div>`;
}

function stripNestedTransclusion(source: string): string {
  return source.replace(TRANSCLUSION_RE, (_match, target: string) => `[[${target}]]`);
}

/** Replace transclusion tokens with rendered embedded note HTML. */
export async function resolveTransclusions(
  source: string,
  context: {
    sourcePath: string | null;
    notes: VaultNote[];
    selectedPath: string | null;
    selectedContent: string;
    labelByPath: Map<string, string>;
  },
): Promise<string> {
  if (!hasTransclusionBlocks(source)) return source;

  const blocks = extractTransclusionBlocks(source);
  let resolved = source;

  for (const block of blocks) {
    const resolvedPath = resolveWikilinkTarget(
      block.target,
      context.sourcePath,
      context.notes,
    );
    if (!resolvedPath) {
      resolved = resolved.replace(
        block.fullMatch,
        renderTransclusionError(`Note not found: ${block.target}`),
      );
      continue;
    }

    const content = await loadNoteContent(
      resolvedPath,
      context.selectedPath,
      context.selectedContent,
    );
    if (content == null) {
      resolved = resolved.replace(
        block.fullMatch,
        renderTransclusionError(`Could not load: ${block.target}`),
      );
      continue;
    }

    let body = stripFrontmatter(content).content;
    body = stripNestedTransclusion(body);

    const label =
      context.labelByPath.get(resolvedPath) ??
      vaultDisplayTitle(resolvedPath.split("/").pop()?.replace(/\.md$/i, "") ?? resolvedPath, resolvedPath);

    const html = renderMarkdown(body, {
      titleByPath: context.labelByPath,
      sourcePath: resolvedPath,
      knownPaths: new Set(context.notes.map((note) => note.path)),
    });

    const headingAttr = block.heading
      ? ` data-transclude-heading="${escapeAttr(block.heading)}"`
      : "";

    resolved = resolved.replace(
      block.fullMatch,
      `<aside class="markdown-transclusion" data-transclude-path="${escapeAttr(resolvedPath)}"${headingAttr}><header class="markdown-transclusion-header"><span class="markdown-transclusion-label">${escapeHtml(label)}</span><button type="button" class="markdown-transclusion-open" data-open-vault-note="${escapeAttr(resolvedPath)}">Open</button></header><div class="markdown-transclusion-body markdown-content">${html}</div></aside>`,
    );
  }

  return resolved;
}
