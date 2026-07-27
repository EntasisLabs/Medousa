import type { VaultNote } from "$lib/types/vault";
import type { ContextMapEdge, ContextMapNode } from "$lib/utils/contextMap";
import { isWorkshopVaultTag } from "$lib/utils/vaultFrontmatter";

export const MAX_NOTES = 80;
export const MAX_TAG_EDGES_PER_NOTE = 4;
export const HUB_TAG_NOTE_LIMIT = 12;

export function noteMapId(path: string): string {
  return `note:${path}`;
}

export function notePathFromMapId(nodeId: string): string | null {
  if (!nodeId.startsWith("note:")) return null;
  return nodeId.slice("note:".length);
}

/** Extract `chat:` prefixes from note tags (normalized lowercase). */
export function chatPrefixesFromTags(tags: string[]): string[] {
  const out: string[] = [];
  for (const tag of tags) {
    const t = tag.trim().toLowerCase();
    if (!t.startsWith("chat:")) continue;
    const prefix = t.slice("chat:".length).trim();
    if (prefix && !out.includes(prefix)) out.push(prefix);
  }
  return out;
}

/**
 * Match a note's `chat:{first8}` tag to a visible session id.
 * Prefers the longest matching prefix / exact start.
 */
export function sessionIdForNoteChatTag(
  tags: string[],
  sessionIds: Iterable<string>,
): string | null {
  const prefixes = chatPrefixesFromTags(tags);
  if (prefixes.length === 0) return null;

  const sessions = [...sessionIds];
  let best: { sessionId: string; len: number } | null = null;

  for (const prefix of prefixes) {
    for (const sessionId of sessions) {
      const id = sessionId.trim();
      if (!id) continue;
      const lower = id.toLowerCase();
      if (lower === prefix || lower.startsWith(prefix)) {
        if (!best || prefix.length > best.len) {
          best = { sessionId: id, len: prefix.length };
        }
      }
    }
  }

  return best?.sessionId ?? null;
}

function humanTags(tags: string[]): string[] {
  const out: string[] = [];
  for (const tag of tags) {
    const t = tag.trim().toLowerCase();
    if (!t || isWorkshopVaultTag(t)) continue;
    if (!out.includes(t)) out.push(t);
  }
  return out;
}

function noteSearchText(note: VaultNote): string {
  return [note.title, note.path, ...note.tags].join(" ").toLowerCase();
}

function noteMatchesSearch(note: VaultNote, needle: string): boolean {
  if (!needle) return true;
  return noteSearchText(note).includes(needle);
}

function pairKey(a: string, b: string): string {
  return a < b ? `${a}|${b}` : `${b}|${a}`;
}

function truncateLabel(text: string, max: number): string {
  const trimmed = text.trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max - 1)}…`;
}

function noteHue(path: string): number {
  let hash = 0;
  for (let i = 0; i < path.length; i += 1) {
    hash = (hash * 33 + path.charCodeAt(i)) >>> 0;
  }
  return hash % 8;
}

function parseModified(value: string): number {
  const ms = Date.parse(value);
  return Number.isNaN(ms) ? 0 : ms;
}

export interface NoteGraphSlice {
  nodes: ContextMapNode[];
  edges: ContextMapEdge[];
}

/**
 * Build note nodes + note_session / note_link / note_tag edges for the map.
 * `sessionIds` are raw chat session ids (not `session:` prefixed).
 */
export function buildNoteGraphSlice(
  notes: VaultNote[],
  sessionIds: string[],
  options?: {
    maxNotes?: number;
    searchQuery?: string;
    labelMax?: number;
    sessionNodeExists?: (sessionId: string) => boolean;
  },
): NoteGraphSlice {
  const maxNotes = options?.maxNotes ?? MAX_NOTES;
  const needle = options?.searchQuery?.trim().toLowerCase() ?? "";
  const labelMax = options?.labelMax ?? 28;
  const sessionExists =
    options?.sessionNodeExists ?? ((id: string) => sessionIds.includes(id));

  const sessionSet = new Set(sessionIds);
  const byPath = new Map(notes.map((note) => [note.path, note]));

  type Ranked = { note: VaultNote; score: number; linkedSession: string | null };
  const ranked: Ranked[] = notes.map((note) => {
    const linkedSession = sessionIdForNoteChatTag(note.tags, sessionSet);
    let score = parseModified(note.modified_at_utc) / 1e13;
    if (linkedSession && sessionExists(linkedSession)) score += 100;
    if (note.wikilinks_out.length > 0 || note.backlinks.length > 0) score += 10;
    if (humanTags(note.tags).length > 0) score += 5;
    if (needle && noteMatchesSearch(note, needle)) score += 50;
    else if (needle) score -= 1000;
    return { note, score, linkedSession };
  });

  ranked.sort((left, right) => right.score - left.score);

  // Prefer session-linked / searchable notes, then fill by recency.
  const selected: Ranked[] = [];
  const selectedPaths = new Set<string>();

  for (const entry of ranked) {
    if (selected.length >= maxNotes) break;
    if (needle && !noteMatchesSearch(entry.note, needle) && !entry.linkedSession) {
      continue;
    }
    if (needle && !noteMatchesSearch(entry.note, needle)) {
      // Keep session-linked notes only when their session is on the map and search is empty for them
      // — under search, require title/path/tag match.
      continue;
    }
    selected.push(entry);
    selectedPaths.add(entry.note.path);
  }

  // Grow via wikilinks among candidates until cap.
  let grew = true;
  while (grew && selected.length < maxNotes) {
    grew = false;
    for (const entry of [...selected]) {
      const neighbors = [
        ...entry.note.wikilinks_out,
        ...entry.note.backlinks,
      ];
      for (const path of neighbors) {
        if (selected.length >= maxNotes) break;
        if (selectedPaths.has(path)) continue;
        const note = byPath.get(path);
        if (!note) continue;
        if (needle && !noteMatchesSearch(note, needle)) continue;
        const linkedSession = sessionIdForNoteChatTag(note.tags, sessionSet);
        selected.push({ note, score: 0, linkedSession });
        selectedPaths.add(path);
        grew = true;
      }
    }
  }

  const nodes: ContextMapNode[] = [];
  const edges: ContextMapEdge[] = [];
  const noteIds = new Set<string>();

  for (const entry of selected) {
    const id = noteMapId(entry.note.path);
    noteIds.add(id);
    const linked = entry.linkedSession;
    nodes.push({
      id,
      kind: "note",
      label: truncateLabel(entry.note.title || entry.note.path, labelMax),
      sessionId: linked ?? "",
      notePath: entry.note.path,
      x: 0,
      y: 0,
      radius: 7,
      weight: 1.2 + Math.min(3, humanTags(entry.note.tags).length * 0.2),
      hue: noteHue(entry.note.path),
      visible: true,
      showLabel: false,
      renderMode: "full",
    });

    if (linked && sessionExists(linked)) {
      edges.push({
        id: `note_session:${id}:session:${linked}`,
        from: id,
        to: `session:${linked}`,
        kind: "note_session",
        visible: true,
        strength: 0.1,
      });
    }
  }

  // Wikilinks (undirected for layout).
  const linkSeen = new Set<string>();
  for (const entry of selected) {
    const fromId = noteMapId(entry.note.path);
    const targets = new Set([...entry.note.wikilinks_out, ...entry.note.backlinks]);
    for (const targetPath of targets) {
      const toId = noteMapId(targetPath);
      if (!noteIds.has(toId) || toId === fromId) continue;
      const key = pairKey(fromId, toId);
      if (linkSeen.has(key)) continue;
      linkSeen.add(key);
      edges.push({
        id: `note_link:${key}`,
        from: fromId,
        to: toId,
        kind: "note_link",
        visible: true,
        strength: 0.12,
      });
    }
  }

  // Shared human tags — skip hubs, cap per note.
  const tagToNotes = new Map<string, string[]>();
  for (const entry of selected) {
    const id = noteMapId(entry.note.path);
    for (const tag of humanTags(entry.note.tags)) {
      const list = tagToNotes.get(tag) ?? [];
      list.push(id);
      tagToNotes.set(tag, list);
    }
  }

  const usableTags = [...tagToNotes.entries()]
    .filter(([, list]) => list.length >= 2 && list.length <= HUB_TAG_NOTE_LIMIT)
    .sort((left, right) => left[1].length - right[1].length);

  const tagEdgeCount = new Map<string, number>();
  const tagSeen = new Set<string>();

  for (const [tag, list] of usableTags) {
    const rarity = 1 / list.length;
    for (let i = 0; i < list.length; i += 1) {
      for (let j = i + 1; j < list.length; j += 1) {
        const a = list[i];
        const b = list[j];
        if ((tagEdgeCount.get(a) ?? 0) >= MAX_TAG_EDGES_PER_NOTE) continue;
        if ((tagEdgeCount.get(b) ?? 0) >= MAX_TAG_EDGES_PER_NOTE) continue;
        const key = pairKey(a, b);
        if (tagSeen.has(key) || linkSeen.has(key)) continue;
        tagSeen.add(key);
        tagEdgeCount.set(a, (tagEdgeCount.get(a) ?? 0) + 1);
        tagEdgeCount.set(b, (tagEdgeCount.get(b) ?? 0) + 1);
        edges.push({
          id: `note_tag:${tag}:${key}`,
          from: a,
          to: b,
          kind: "note_tag",
          visible: true,
          strength: 0.02 + rarity * 0.03,
        });
      }
    }
  }

  return { nodes, edges };
}
