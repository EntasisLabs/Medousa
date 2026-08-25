import {
  listVaultChanges,
  listVaultNotes,
  listVaultTags,
  searchVaultNotes,
} from "$lib/daemon";
import {
  resolveSpaceForPath,
  saveLastSpace,
  saveShowSystemNotes,
  shouldHideGarageNote,
} from "$lib/config/vaultSpaces";
import type { VaultNote, VaultSearchHit, VaultTreeNode } from "$lib/types/vault";
import { buildVaultTree } from "$lib/utils/vaultTree";
import { resolveKind, sortVaultTagsForDisplay, type VaultNoteKind } from "$lib/utils/vaultFrontmatter";
import {
  VAULT_LIST_MAX_PAGES,
  VAULT_LIST_PAGE_LIMIT,
  listingIncompleteAfterPages,
} from "$lib/utils/vaultListing";
import { VAULT_NOTES_REFRESH_MS } from "$lib/utils/vaultSave";
import { addCustomVaultSpace } from "$lib/utils/vaultCustomSpaces";

const LIBRARY_BROWSE_MODE_KEY = "medousa-home-vault-browse-mode";
const RECENT_BROWSE_LIMIT = 40;
const AGENT_WRITE_TTL_MS = 24 * 60 * 60 * 1000;
const KIND_BROWSE_ORDER: VaultNoteKind[] = [
  "daily",
  "project",
  "ledger",
  "workbook",
  "sheet",
  "board",
  "slides",
  "draw",
  "resume",
  "inbox",
  "bug",
  "note",
];

export type LibraryBrowseMode = "folders" | "tags" | "recent" | "kind";
export type VaultTagCount = { tag: string; count: number };

const LIBRARY_BROWSE_MODES = new Set<LibraryBrowseMode>([
  "folders",
  "tags",
  "recent",
  "kind",
]);

export function loadLibraryBrowseMode(): LibraryBrowseMode {
  if (typeof localStorage === "undefined") return "recent";
  try {
    const raw = localStorage.getItem(LIBRARY_BROWSE_MODE_KEY);
    if (raw && LIBRARY_BROWSE_MODES.has(raw as LibraryBrowseMode)) {
      return raw as LibraryBrowseMode;
    }
  } catch {
    /* ignore */
  }
  return "recent";
}

function saveLibraryBrowseMode(mode: LibraryBrowseMode) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(LIBRARY_BROWSE_MODE_KEY, mode);
  } catch {
    /* ignore */
  }
}

export function isRecentAgentWrite(
  path: string,
  agentWrittenAt: Record<string, string>,
): boolean {
  const writtenAt = agentWrittenAt[path];
  if (!writtenAt) return false;
  return Date.now() - Date.parse(writtenAt) < AGENT_WRITE_TTL_MS;
}

export type VaultBrowseHost = {
  notes: VaultNote[];
  tree: VaultTreeNode[];
  showSystemNotes: boolean;
  activeSpaceFilter: string | null;
  showAgentReviewFilter: boolean;
  agentWrittenAt: Record<string, string>;
  libraryBrowseMode: LibraryBrowseMode;
  vaultTags: VaultTagCount[];
  vaultGeneration: number;
  listingIncomplete: boolean;
  error: string | null;
  searchQuery: string;
  searchHits: VaultSearchHit[];
  recentPaths: string[];
  workshopEpoch: number;
  rebuildLookupSnapshot(): void;
};

export class VaultBrowseController {
  #host: VaultBrowseHost;
  #notesRefreshTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(host: VaultBrowseHost) {
    this.#host = host;
  }

  resetForWorkshopSwitch() {
    if (this.#notesRefreshTimer) clearTimeout(this.#notesRefreshTimer);
    this.#notesRefreshTimer = null;
  }

  rebuildTree() {
    const host = this.#host;
    host.tree = buildVaultTree(host.notes, {
      showSystemNotes: host.showSystemNotes,
      spaceFilter: host.activeSpaceFilter,
      agentReviewOnly: host.showAgentReviewFilter,
      agentWrittenAt: host.agentWrittenAt,
    });
    this.rebuildVaultTagsFromNotes();
  }

  setLibraryBrowseMode(mode: LibraryBrowseMode) {
    this.#host.libraryBrowseMode = mode;
    saveLibraryBrowseMode(mode);
    if (mode === "tags") {
      void this.refreshVaultTags();
    }
  }

  scopedLibraryNotes(): VaultNote[] {
    const host = this.#host;
    const agentMap = host.agentWrittenAt;
    const agentOnly = host.showAgentReviewFilter;
    const showSystem = host.showSystemNotes;
    const spaceFilter = host.activeSpaceFilter;
    return host.notes.filter((note) => {
      if (agentOnly && !isRecentAgentWrite(note.path, agentMap)) return false;
      if (!showSystem && shouldHideGarageNote(note.path, note.title, showSystem)) {
        return false;
      }
      if (spaceFilter) {
        return resolveSpaceForPath(note.path, note.title).id === spaceFilter;
      }
      return true;
    });
  }

  notesForTag(tag: string): VaultNote[] {
    const needle = tag.trim().toLowerCase();
    if (!needle) return [];
    return this.scopedLibraryNotes()
      .filter((note) =>
        (note.tags ?? []).some((entry) => entry.trim().toLowerCase() === needle),
      )
      .sort((a, b) => a.title.localeCompare(b.title));
  }

  notesByKind(): { kind: VaultNoteKind; notes: VaultNote[] }[] {
    const buckets = new Map<VaultNoteKind, VaultNote[]>();
    for (const kind of KIND_BROWSE_ORDER) {
      buckets.set(kind, []);
    }
    for (const note of this.scopedLibraryNotes()) {
      const kind = resolveKind(note.path, note.kind);
      const bucket = buckets.get(kind) ?? buckets.get("note")!;
      bucket.push(note);
    }
    return KIND_BROWSE_ORDER.map((kind) => ({
      kind,
      notes: (buckets.get(kind) ?? []).sort((a, b) => a.title.localeCompare(b.title)),
    })).filter((group) => group.notes.length > 0);
  }

  recentNotesList(limit = RECENT_BROWSE_LIMIT): VaultNote[] {
    const scoped = this.scopedLibraryNotes();
    const byPath = new Map(scoped.map((note) => [note.path, note]));
    const result: VaultNote[] = [];
    const seen = new Set<string>();
    for (const path of this.#host.recentPaths) {
      const note = byPath.get(path);
      if (!note) continue;
      result.push(note);
      seen.add(path);
      if (result.length >= limit) return result;
    }
    const rest = [...scoped]
      .filter((note) => !seen.has(note.path))
      .sort(
        (a, b) =>
          Date.parse(b.modified_at_utc || "0") - Date.parse(a.modified_at_utc || "0"),
      );
    for (const note of rest) {
      result.push(note);
      if (result.length >= limit) break;
    }
    return result;
  }

  rebuildVaultTagsFromNotes(extraTags: string[] = []) {
    const counts = new Map<string, number>();
    for (const note of this.scopedLibraryNotes()) {
      for (const tag of note.tags ?? []) {
        const trimmed = tag.trim();
        if (!trimmed) continue;
        counts.set(trimmed, (counts.get(trimmed) ?? 0) + 1);
      }
    }
    for (const tag of extraTags) {
      const trimmed = tag.trim();
      if (!trimmed || counts.has(trimmed)) continue;
      counts.set(trimmed, 0);
    }
    this.#host.vaultTags = sortVaultTagsForDisplay([...counts.keys()])
      .map((tag) => ({ tag, count: counts.get(tag) ?? 0 }))
      .filter((row) => row.count > 0);
  }

  async refreshVaultTags() {
    const workshopEpoch = this.#host.workshopEpoch;
    try {
      const response = await listVaultTags({ limit: 500 });
      if (workshopEpoch !== this.#host.workshopEpoch) return;
      this.rebuildVaultTagsFromNotes(response.tags ?? []);
    } catch {
      if (workshopEpoch !== this.#host.workshopEpoch) return;
      this.rebuildVaultTagsFromNotes();
    }
  }

  setShowAgentReviewFilter(value: boolean) {
    this.#host.showAgentReviewFilter = value;
    this.rebuildTree();
  }

  setShowSystemNotes(value: boolean) {
    this.#host.showSystemNotes = value;
    saveShowSystemNotes(value);
    this.rebuildTree();
  }

  setActiveSpaceFilter(spaceId: string | null) {
    this.#host.activeSpaceFilter = spaceId;
    saveLastSpace(spaceId);
    this.rebuildTree();
    if (this.#host.searchQuery.trim()) {
      void this.runSearch(this.#host.searchQuery);
    }
  }

  focusSpaceForPath(path: string, title: string) {
    const space = resolveSpaceForPath(path, title);
    if (space.id === "system_bucket" || space.id === "other") {
      this.setActiveSpaceFilter(null);
      return;
    }
    this.setActiveSpaceFilter(space.id);
    saveLastSpace(space.id);
  }

  applySpaceFilterAfterMove(path: string, title: string, filterWasAll: boolean) {
    if (filterWasAll) return;
    this.focusSpaceForPath(path, title);
  }

  addCustomGroup(label: string) {
    const space = addCustomVaultSpace(label);
    if (space) {
      this.rebuildTree();
      this.setActiveSpaceFilter(space.id);
    }
    return space;
  }

  scheduleNotesRefresh() {
    if (this.#notesRefreshTimer) {
      clearTimeout(this.#notesRefreshTimer);
    }
    this.#notesRefreshTimer = setTimeout(() => {
      this.#notesRefreshTimer = null;
      void this.refreshNotes();
    }, VAULT_NOTES_REFRESH_MS);
  }

  async refreshNotes() {
    const host = this.#host;
    const workshopEpoch = host.workshopEpoch;
    host.error = null;
    try {
      if (host.vaultGeneration > 0) {
        const delta = await listVaultChanges({
          sinceGeneration: host.vaultGeneration,
          limit: 500,
        });
        if (workshopEpoch !== host.workshopEpoch) return;
        if (!delta.reset_required && delta.changes.every((change) => change.kind === "delete")) {
          const removed = new Set(delta.changes.map((change) => change.path));
          host.notes = host.notes.filter((note) => !removed.has(note.path));
          host.vaultGeneration = delta.vault_generation;
          host.listingIncomplete = false;
          host.rebuildLookupSnapshot();
          this.rebuildTree();
          if (host.libraryBrowseMode === "tags") {
            void this.refreshVaultTags();
          }
          return;
        }
      }

      const pageLimit = VAULT_LIST_PAGE_LIMIT;
      const notes: VaultNote[] = [];
      let cursor: string | undefined;
      let generation: number | undefined;
      let incomplete = false;
      for (let page = 0; page < VAULT_LIST_MAX_PAGES; page += 1) {
        const response = await listVaultNotes({
          limit: pageLimit,
          cursor,
          generation,
        });
        if (workshopEpoch !== host.workshopEpoch) return;
        if (response.reset_required) {
          notes.length = 0;
          cursor = undefined;
          generation = response.vault_generation ?? undefined;
          continue;
        }
        for (const note of response.notes) {
          notes.push({
            path: note.path,
            title: note.title,
            byte_size: 0,
            content_hash: "",
            modified_at_utc: note.modified_at_utc,
            created_at_utc: note.modified_at_utc,
            tags: note.tags ?? [],
            wikilinks_out: [],
            backlinks: [],
            kind: note.kind,
          });
        }
        generation = response.vault_generation ?? generation;
        if (!response.truncated || !response.next_cursor) {
          host.vaultGeneration = generation ?? host.vaultGeneration + 1;
          incomplete = false;
          break;
        }
        if (
          listingIncompleteAfterPages(
            page + 1,
            Boolean(response.truncated),
            response.next_cursor,
          )
        ) {
          incomplete = true;
          break;
        }
        cursor = response.next_cursor;
      }
      if (incomplete) {
        host.listingIncomplete = true;
        host.error = "Vault listing is incomplete; page until the listing finishes.";
        return;
      }
      host.listingIncomplete = false;
      host.notes = notes;
      host.rebuildLookupSnapshot();
      this.rebuildTree();
      if (host.libraryBrowseMode === "tags") {
        void this.refreshVaultTags();
      }
    } catch (err) {
      if (workshopEpoch !== host.workshopEpoch) return;
      host.error = err instanceof Error ? err.message : String(err);
    }
  }

  async runSearch(query: string) {
    const host = this.#host;
    const workshopEpoch = host.workshopEpoch;
    host.searchQuery = query;
    if (!query.trim()) {
      host.searchHits = [];
      return;
    }
    try {
      const response = await searchVaultNotes(query.trim(), 20);
      if (workshopEpoch !== host.workshopEpoch) return;
      let hits = response.hits;
      if (host.activeSpaceFilter) {
        hits = hits.filter((hit) => {
          const title = hit.note.title;
          return resolveSpaceForPath(hit.note.path, title).id === host.activeSpaceFilter;
        });
      }
      if (!host.showSystemNotes) {
        hits = hits.filter(
          (hit) => !shouldHideGarageNote(hit.note.path, hit.note.title, host.showSystemNotes),
        );
      }
      host.searchHits = hits.slice(0, 12);
    } catch (err) {
      if (workshopEpoch !== host.workshopEpoch) return;
      host.error = err instanceof Error ? err.message : String(err);
    }
  }
}
