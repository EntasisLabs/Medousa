/** Index vault notes that represent saved browser pages. */

import { getVaultNote, searchVaultNotes } from "$lib/daemon";
import type { VaultNote } from "$lib/types/vault";
import { normalizeBrowserUrl } from "$lib/utils/browserUrl";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";

export type VaultBrowserSave = {
  url: string;
  title: string;
  path: string;
  savedAt: string | null;
};

const LEGACY_SOURCE_RE = /^Source:\s*(.+)$/im;

function parseFrontmatterField(
  frontmatter: string | null,
  key: string,
): string | null {
  if (!frontmatter) return null;
  const prefix = `${key}:`;
  for (const line of frontmatter.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith(prefix)) {
      const value = trimmed.slice(prefix.length).trim();
      return value.replace(/^['"]|['"]$/g, "") || null;
    }
  }
  return null;
}

export function extractSourceFromNoteContent(content: string): string | null {
  const { frontmatter, content: body } = stripFrontmatter(content);
  const fromFrontmatter = parseFrontmatterField(frontmatter, "source");
  if (fromFrontmatter) return fromFrontmatter;

  const legacy = body.match(LEGACY_SOURCE_RE);
  return legacy?.[1]?.trim() ?? null;
}

function noteTitle(note: VaultNote, content: string): string {
  const trimmed = note.title?.trim();
  if (trimmed) return trimmed;
  const { content: body } = stripFrontmatter(content);
  const heading = body.match(/^#\s+(.+)$/m);
  return heading?.[1]?.trim() ?? note.path;
}

export async function findVaultNoteBySource(url: string): Promise<string | null> {
  const norm = normalizeBrowserUrl(url);
  if (!norm || norm === "about:blank") return null;

  try {
    const tagged = await searchVaultNotes(norm, 12, ["bookmark"]);
    for (const hit of tagged.hits) {
      const response = await getVaultNote(hit.note.path);
      const source = extractSourceFromNoteContent(response.content);
      if (source && normalizeBrowserUrl(source) === norm) {
        return hit.note.path;
      }
    }

    const legacy = await searchVaultNotes(`Source: ${url}`, 8);
    for (const hit of legacy.hits) {
      const response = await getVaultNote(hit.note.path);
      const source = extractSourceFromNoteContent(response.content);
      if (source && normalizeBrowserUrl(source) === norm) {
        return hit.note.path;
      }
    }
  } catch {
    return null;
  }

  return null;
}

export async function listVaultBrowserSaves(notes: VaultNote[]): Promise<VaultBrowserSave[]> {
  const bookmarkNotes = notes.filter((note) => note.tags.includes("bookmark"));
  const saves: VaultBrowserSave[] = [];
  const seen = new Set<string>();

  for (const note of bookmarkNotes) {
    try {
      const response = await getVaultNote(note.path);
      const source = extractSourceFromNoteContent(response.content);
      if (!source) continue;
      const norm = normalizeBrowserUrl(source);
      if (seen.has(norm)) continue;
      seen.add(norm);

      const { frontmatter } = stripFrontmatter(response.content);
      saves.push({
        url: source,
        title: noteTitle(note, response.content),
        path: note.path,
        savedAt: parseFrontmatterField(frontmatter, "saved_at") ?? note.modified_at_utc,
      });
    } catch {
      continue;
    }
  }

  try {
    const legacy = await searchVaultNotes("Source:", 24);
    for (const hit of legacy.hits) {
      if (hit.note.path.endsWith(".md") === false) continue;
      if (bookmarkNotes.some((note) => note.path === hit.note.path)) continue;
      try {
        const response = await getVaultNote(hit.note.path);
        const source = extractSourceFromNoteContent(response.content);
        if (!source) continue;
        const norm = normalizeBrowserUrl(source);
        if (seen.has(norm)) continue;
        if (!response.content.match(LEGACY_SOURCE_RE)) continue;
        seen.add(norm);
        saves.push({
          url: source,
          title: noteTitle(hit.note as VaultNote, response.content),
          path: hit.note.path,
          savedAt: hit.note.modified_at_utc,
        });
      } catch {
        continue;
      }
    }
  } catch {
    // ignore search failures
  }

  return saves.sort(
    (a, b) => Date.parse(b.savedAt ?? "") - Date.parse(a.savedAt ?? ""),
  );
}

export function bookmarkNoteContent(
  title: string,
  url: string,
  savedAt = new Date(),
): string {
  const safeTitle = title.trim() || url;
  const iso = savedAt.toISOString();
  return `---\nkind: note\ntags: [bookmark]\nsource: ${url}\nsaved_at: ${iso}\n---\n\n# ${safeTitle}\n`;
}
