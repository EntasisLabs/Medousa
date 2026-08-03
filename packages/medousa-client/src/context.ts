import type { HostTurnContext, MedousaContext } from "./types.js";

const MAX_SELECTION_CHARS = 12_000;
const MAX_PAGE_TEXT_CHARS = 24_000;
const MAX_DIAGNOSTICS = 100;

/**
 * Bound host context before it crosses the plugin/daemon boundary.
 * Host adapters should call this at the edge, before adding context to a turn.
 */
export function boundContext(context: MedousaContext): MedousaContext {
  return {
    ...context,
    selection: context.selection
      ? {
          ...context.selection,
          text: context.selection.text.slice(0, MAX_SELECTION_CHARS),
        }
      : undefined,
    pageText: context.pageText?.slice(0, MAX_PAGE_TEXT_CHARS),
    documentExcerpt: context.documentExcerpt?.slice(0, MAX_PAGE_TEXT_CHARS),
    diagnostics: context.diagnostics?.slice(0, MAX_DIAGNOSTICS),
  };
}

/**
 * Convert adapter-friendly context into the canonical daemon contract.
 */
export function hostContext(context: MedousaContext): HostTurnContext {
  const bounded = boundContext(context);
  return {
    source: bounded.surface,
    workspace: bounded.workspace,
    resource_kind: bounded.url ? "page" : bounded.notePath ? "note" : bounded.file ? "file" : undefined,
    resource_path: bounded.notePath ?? bounded.file,
    resource_title: bounded.title,
    resource_url: bounded.url,
    language: bounded.language,
    cursor: bounded.cursor,
    selection: bounded.selection,
    document_excerpt: bounded.documentExcerpt ?? bounded.pageText,
    diagnostics: bounded.diagnostics?.map((item) => ({
      message: item.message,
      severity: item.severity,
      source: item.source,
      start: item.range?.start,
      end: item.range?.end,
    })) ?? [],
    related_resources: bounded.relatedResources ?? [],
  };
}
