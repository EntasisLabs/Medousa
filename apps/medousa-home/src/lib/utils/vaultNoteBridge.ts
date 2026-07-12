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
  path: string,
  title: string,
  content: string,
  linkCount = 0,
): string {
  const label = vaultDisplayTitle(title, path);
  const excerpt = noteExcerpt(content);
  const linkHint =
    linkCount > 0
      ? `\n\nAlso consider the ${linkCount} linked note${linkCount === 1 ? "" : "s"} from this page.`
      : "";
  return `I'm reading my vault note "${label}" (\`${path}\`).\n\n${excerpt}${linkHint}\n\nHelp me think through this note.`;
}

export function buildAskAboutSelectionDraft(
  path: string,
  title: string,
  selectedText: string,
): string {
  const label = vaultDisplayTitle(title, path);
  const excerpt = noteExcerpt(selectedText, 2000);
  return `Looking at this passage in "${label}" (\`${path}\`):\n\n${excerpt}\n\nHelp me with this — edit, clarify, or suggest next.`;
}

/** Keep the passage visible to Medousa even if the composer draft drifted. */
export function ensureVaultSelectionInPrompt(
  prompt: string,
  scope: VaultNoteContextScope | null | undefined,
): string {
  if (!vaultContextHasSelection(scope)) return prompt;
  const text = scope!.selection!.text.trim();
  if (!text) return prompt;
  if (prompt.includes(text)) return prompt;
  const block = buildAskAboutSelectionDraft(scope!.path, scope!.title, text);
  const trimmed = prompt.trim();
  if (!trimmed) return block;
  return `${block}\n\n---\n\n${trimmed}`;
}

export function buildWorkAskFromNote(
  path: string,
  title: string,
  content: string,
): string {
  const label = vaultDisplayTitle(title, path);
  const excerpt = noteExcerpt(content, 2000);
  return `Work from my vault note "${label}" (\`${path}\`).\n\n${excerpt}\n\nUse this note as primary context and report back with concrete next steps.`;
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
