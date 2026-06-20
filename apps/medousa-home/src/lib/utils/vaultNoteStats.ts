/** Vault note word / character stats (body text, frontmatter excluded). */

import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";

export interface VaultNoteStats {
  words: number;
  characters: number;
  charactersNoSpaces: number;
  /** Rough reading time at ~200 wpm, minimum 1 when non-empty. */
  readMinutes: number;
}

export function vaultNoteStats(content: string): VaultNoteStats {
  const body = stripFrontmatter(content).content;
  const trimmed = body.trim();
  const words = trimmed ? trimmed.split(/\s+/).filter(Boolean).length : 0;
  const characters = body.length;
  const charactersNoSpaces = body.replace(/\s/g, "").length;
  const readMinutes = words === 0 ? 0 : Math.max(1, Math.ceil(words / 200));

  return { words, characters, charactersNoSpaces, readMinutes };
}

export function formatVaultNoteStats(stats: VaultNoteStats): string {
  if (stats.words === 0 && stats.characters === 0) {
    return "Empty note";
  }
  const readLabel =
    stats.readMinutes === 0
      ? ""
      : stats.readMinutes === 1
        ? "~1 min read"
        : `~${stats.readMinutes} min read`;
  const wordLabel = stats.words === 1 ? "1 word" : `${stats.words.toLocaleString()} words`;
  return readLabel ? `${wordLabel} · ${readLabel}` : wordLabel;
}
