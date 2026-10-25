/** M7e — vault ↔ chat/work bridge helpers. */

import type { VaultNote } from "$lib/types/vault";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { dailyNotePath, isoDateLocal } from "$lib/utils/vaultTemplates";
import { vaultDisplayTitle } from "$lib/utils/formatVault";

const DAILY_PATH = /^journal\/\d{4}-\d{2}-\d{2}\.md$/;

export interface VaultNoteSelection {
  text: string;
  /** Source offsets when known (edit mode). */
  start?: number;
  end?: number;
}

export interface VaultNoteContextScope {
  rootId?: string;
  workshopId?: string;
  path: string;
  title: string;
  linkCount: number;
  /** Highlighted passage for collab (Add to chat). */
  selection?: VaultNoteSelection;
}

export function buildVaultNoteContextScope(
  path: string,
  title: string,
  wikilinksOut: string[],
  backlinks: string[],
  selection?: VaultNoteSelection | null,
): VaultNoteContextScope {
  const trimmed = selection?.text.trim();
  return {
    path,
    title: vaultDisplayTitle(title, path),
    linkCount: wikilinksOut.length + backlinks.length,
    selection: trimmed
      ? {
          text: trimmed,
          start: selection?.start,
          end: selection?.end,
        }
      : undefined,
  };
}

export function vaultContextHasSelection(
  scope: VaultNoteContextScope | null | undefined,
): boolean {
  return Boolean(scope?.selection?.text.trim());
}

/** UI hint for scoped chat context (D3). */
export function vaultContextScopeHint(scope: VaultNoteContextScope): string {
  if (vaultContextHasSelection(scope)) {
    const preview = scope.selection!.text.replace(/\s+/g, " ").trim();
    return preview.length > 52 ? `${preview.slice(0, 51)}…` : preview;
  }
  if (scope.linkCount === 0) return "This page";
  const n = scope.linkCount;
  return `This page + ${n} linked note${n === 1 ? "" : "s"}`;
}

export function vaultContextScopeLabel(scope: VaultNoteContextScope): string {
  return vaultContextHasSelection(scope) ? "Passage" : "Note";
}

export function prepareTalkAboutNote(
  path: string,
  title: string,
  content: string,
  wikilinksOut: string[],
  backlinks: string[],
): { scope: VaultNoteContextScope; draft: string } {
  const scope = buildVaultNoteContextScope(path, title, wikilinksOut, backlinks);
  return {
    scope,
    draft: buildAskAboutNoteDraft(path, title, content, scope.linkCount),
  };
}

export function prepareAddSelectionToChat(
  path: string,
  title: string,
  selection: VaultNoteSelection,
  wikilinksOut: string[],
  backlinks: string[],
): { scope: VaultNoteContextScope; draft: string } {
  const scope = buildVaultNoteContextScope(
    path,
    title,
    wikilinksOut,
    backlinks,
    selection,
  );
  return {
    scope,
    draft: buildAskAboutSelectionDraft(path, title, scope.selection!.text),
  };
}

export function noteExcerpt(content: string, maxChars = 1200): string {
  const body = stripFrontmatter(content).content.trim();
  if (body.length <= maxChars) return body;
  return `${body.slice(0, maxChars).trimEnd()}…`;
}

export function buildAskAboutNoteDraft(
  _path: string,
  _title: string,
  _content: string,
  _linkCount = 0,
): string {
  return "Help me think through this note.";
}

export function buildAskAboutSelectionDraft(
  _path: string, _title: string, _selectedText: string,
): string { return "Help me with this passage — edit, clarify, or suggest next."; }

export function buildWorkAskFromNote(_path: string, _title: string, _content: string): string {
  return "Use this note as primary context and report back with concrete next steps.";
}

/** Hero target: today's daily, else most recent dated journal note. */
export function resolveJournalDailyHeroPath(notes: VaultNote[]): string | null {
  const today = dailyNotePath();
  if (notes.some((note) => note.path === today)) {
    return today;
  }

  const dated = notes
    .filter((note) => DAILY_PATH.test(note.path))
    .sort((left, right) => right.path.localeCompare(left.path));
  return dated[0]?.path ?? null;
}

/** Most recently modified vault note, for quick resume shortcuts. */
export function resolveLastEditedNote(notes: VaultNote[]): VaultNote | null {
  if (notes.length === 0) return null;
  return [...notes].sort(
    (left, right) =>
      new Date(right.modified_at_utc).getTime() - new Date(left.modified_at_utc).getTime(),
  )[0];
}

export function journalDailyHeroTitle(
  path: string,
  notes: VaultNote[],
  labelByPath: Map<string, string>,
): string {
  const note = notes.find((row) => row.path === path);
  return (
    labelByPath.get(path) ??
    vaultDisplayTitle(note?.title ?? `Daily · ${isoDateLocal()}`, path)
  );
}
