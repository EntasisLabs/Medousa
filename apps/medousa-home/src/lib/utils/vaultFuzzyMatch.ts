/** Lightweight fuzzy scoring for vault quick switcher. */

import type { VaultNote } from "$lib/types/vault";

function fuzzyScore(query: string, text: string): number {
  if (!query) return 1;
  if (text.startsWith(query)) return 200 + query.length;
  if (text.includes(query)) return 120 + query.length;

  let queryIndex = 0;
  let streak = 0;
  let score = 0;
  for (let i = 0; i < text.length && queryIndex < query.length; i += 1) {
    if (text[i] === query[queryIndex]) {
      queryIndex += 1;
      streak += 1;
      score += 10 + streak;
    } else {
      streak = 0;
    }
  }
  return queryIndex === query.length ? score : 0;
}

export function fuzzyMatchVaultNotes(
  notes: VaultNote[],
  query: string,
  labelByPath: Map<string, string>,
  limit = 24,
): VaultNote[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) {
    return notes.slice(0, limit);
  }

  return notes
    .map((note) => {
      const label = (labelByPath.get(note.path) ?? note.title).toLowerCase();
      const path = note.path.toLowerCase();
      const score = Math.max(
        fuzzyScore(trimmed, label),
        fuzzyScore(trimmed, path) * 0.92,
        fuzzyScore(trimmed, label.replace(/\s+/g, "")),
      );
      return { note, score };
    })
    .filter((row) => row.score > 0)
    .sort((left, right) => right.score - left.score || left.note.path.localeCompare(right.note.path))
    .slice(0, limit)
    .map((row) => row.note);
}
