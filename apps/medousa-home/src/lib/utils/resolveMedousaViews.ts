import { getVaultNote } from "$lib/daemon";
import type { VaultNote } from "$lib/types/vault";
import { vaultDisplayTitle } from "$lib/utils/formatVault";
import {
  applyMedousaViewQuery,
  extractMedousaViewBlocks,
  findTableForView,
  hasMedousaViewBlocks,
  renderMedousaViewError,
  renderMedousaViewTable,
  resolveViewSourcePath,
  type ResolvedViewTable,
} from "$lib/utils/markdownView";

const CONTENT_CACHE_TTL_MS = 30_000;
const contentCache = new Map<string, { content: string; fetchedAt: number }>();

export function invalidateMedousaViewCache(path?: string) {
  if (path) {
    contentCache.delete(path);
    return;
  }
  contentCache.clear();
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

async function resolveViewBlock(
  block: ReturnType<typeof extractMedousaViewBlocks>[number],
  context: {
    sourcePath: string | null;
    notes: VaultNote[];
    selectedPath: string | null;
    selectedContent: string;
    labelByPath: Map<string, string>;
  },
): Promise<string> {
  if (!block.query) {
    return renderMedousaViewError("Invalid medousa-view block — check from, where, sort, and columns.");
  }

  const sourcePath = resolveViewSourcePath(
    block.query.from,
    context.sourcePath,
    context.notes,
  );
  if (!sourcePath) {
    return renderMedousaViewError(`Could not resolve source note: ${block.query.from}`);
  }

  const content = await loadNoteContent(
    sourcePath,
    context.selectedPath,
    context.selectedContent,
  );
  if (content == null) {
    return renderMedousaViewError(`Source note not found: ${block.query.from}`);
  }

  const table = findTableForView(content, block.query.table);
  if (!table) {
    return renderMedousaViewError(
      block.query.table === "ledger"
        ? `No ledger table in ${block.query.from}`
        : `No table in ${block.query.from}`,
    );
  }

  const { headers, rows } = applyMedousaViewQuery(table, block.query);
  const resolved: ResolvedViewTable = {
    headers,
    rows,
    sourcePath,
    sourceLabel:
      context.labelByPath.get(sourcePath) ??
      vaultDisplayTitle(sourcePath.split("/").pop()?.replace(/\.md$/i, "") ?? sourcePath, sourcePath),
    query: block.query,
  };
  return renderMedousaViewTable(resolved);
}

/** Replace `medousa-view` fences with live query result HTML. */
export async function resolveMedousaViews(
  source: string,
  context: {
    sourcePath: string | null;
    notes: VaultNote[];
    selectedPath: string | null;
    selectedContent: string;
    labelByPath: Map<string, string>;
  },
): Promise<string> {
  if (!hasMedousaViewBlocks(source)) return source;

  const blocks = extractMedousaViewBlocks(source);
  let resolved = source;
  for (const block of blocks) {
    const html = await resolveViewBlock(block, context);
    resolved = resolved.replace(block.fullMatch, html);
  }
  return resolved;
}
