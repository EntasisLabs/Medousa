import {
  getVaultBacklinks,
  getVaultNote,
  listVaultNotes,
  saveVaultNote,
  searchVaultNotes,
} from "$lib/daemon";
import type {
  VaultNote,
  VaultNoteContentResponse,
  VaultSearchHit,
} from "$lib/types/vault";
import type { VaultTreeNode } from "$lib/types/vault";
import { buildVaultTree } from "$lib/utils/vaultTree";

const LAST_NOTE_KEY = "medousa-home-last-note";

export class VaultStore {
  notes = $state<VaultNote[]>([]);
  tree = $state<VaultTreeNode[]>([]);
  selectedPath = $state<string | null>(loadLastNote());
  content = $state("");
  contentHash = $state<string | null>(null);
  wikilinksOut = $state<string[]>([]);
  backlinks = $state<string[]>([]);
  title = $state("");
  dirty = $state(false);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);
  searchQuery = $state("");
  searchHits = $state<VaultSearchHit[]>([]);
  editorMode = $state<"edit" | "preview">("edit");

  get isDirty(): boolean {
    return this.dirty;
  }

  async refreshNotes() {
    this.loading = true;
    this.error = null;
    try {
      const response = await listVaultNotes(undefined, 500);
      this.notes = response.notes;
      this.tree = buildVaultTree(response.notes);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  async openNote(path: string) {
    this.loading = true;
    this.error = null;
    try {
      const response: VaultNoteContentResponse = await getVaultNote(path);
      this.applyNote(response);
      this.selectedPath = path;
      localStorage.setItem(LAST_NOTE_KEY, path);
      await this.refreshBacklinks(path);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  applyNote(response: VaultNoteContentResponse) {
    this.content = response.content;
    this.contentHash = response.note.content_hash;
    this.title = response.note.title;
    this.wikilinksOut = response.note.wikilinks_out;
    this.backlinks = response.note.backlinks;
    this.dirty = false;
  }

  async refreshBacklinks(path: string) {
    try {
      const response = await getVaultBacklinks(path);
      this.backlinks = response.backlinks;
    } catch {
      // Non-fatal — note metadata may still have backlinks.
    }
  }

  markDirty(nextContent: string) {
    this.content = nextContent;
    this.dirty = true;
  }

  async save() {
    if (!this.selectedPath) return;
    this.saving = true;
    this.error = null;
    try {
      const response = await saveVaultNote(
        this.selectedPath,
        this.content,
        this.contentHash ?? undefined,
      );
      this.contentHash = response.note.content_hash;
      this.title = response.note.title;
      this.wikilinksOut = response.note.wikilinks_out;
      this.dirty = false;
      await this.refreshNotes();
      await this.refreshBacklinks(this.selectedPath);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.saving = false;
    }
  }

  async runSearch(query: string) {
    this.searchQuery = query;
    if (!query.trim()) {
      this.searchHits = [];
      return;
    }
    try {
      const response = await searchVaultNotes(query.trim(), 12);
      this.searchHits = response.hits;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  toggleEditorMode() {
    this.editorMode = this.editorMode === "edit" ? "preview" : "edit";
  }
}

function loadLastNote(): string | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage.getItem(LAST_NOTE_KEY);
}

export const vault = new VaultStore();
