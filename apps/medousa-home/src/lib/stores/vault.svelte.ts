import {
  createVaultNote,
  deleteVaultNote,
  getVaultBacklinks,
  getVaultNote,
  listVaultNotes,
  saveVaultNote,
  searchVaultNotes,
} from "$lib/daemon";
import {
  countNotesBySpace,
  getSpaceById,
  isSystemNoiseNote,
  loadLastSpace,
  loadShowSystemNotes,
  resolveSpaceForPath,
  saveLastSpace,
  saveShowSystemNotes,
} from "$lib/config/vaultSpaces";
import type {
  VaultNote,
  VaultNoteContentResponse,
  VaultSearchHit,
} from "$lib/types/vault";
import type { VaultTreeNode } from "$lib/types/vault";
import { buildVaultLabelMap } from "$lib/utils/formatVault";
import { buildVaultTree } from "$lib/utils/vaultTree";
import {
  contentForTemplate,
  dailyNotePath,
  dailyNoteTemplate,
  inboxCapturePath,
  inboxCaptureTemplate,
  pathForTemplate,
  resolveTemplateForSpace,
  slugifyTitle,
  weeklyReviewPath,
  weeklyReviewTemplate,
  weeklyReviewTitle,
  weeklyReviewWikilink,
  type VaultTemplateId,
} from "$lib/utils/vaultTemplates";
import {
  insertTextAtSection,
  resolveKind,
  setFrontmatterKind,
  type VaultNoteKind,
} from "$lib/utils/vaultFrontmatter";
import { formatDiffChip, lineDiffStats, type LineDiffStats } from "$lib/utils/vaultDiff";
import { resolveWikilinkTarget } from "$lib/utils/resolveWikilink";
import {
  isVaultConflictError,
  VAULT_AUTOSAVE_MS,
  VAULT_SAVED_WHISPER_MS,
  type VaultSaveStatus,
} from "$lib/utils/vaultSave";

const LAST_NOTE_KEY = "medousa-home-last-note";

export class VaultStore {
  notes = $state<VaultNote[]>([]);
  tree = $state<VaultTreeNode[]>([]);
  selectedPath = $state<string | null>(loadLastNote());
  content = $state("");
  baselineContent = $state("");
  contentHash = $state<string | null>(null);
  wikilinksOut = $state<string[]>([]);
  backlinks = $state<string[]>([]);
  title = $state("");
  selectedKind = $state<VaultNoteKind>("note");
  dirty = $state(false);
  saveStatus = $state<VaultSaveStatus>("idle");
  conflictMessage = $state<string | null>(null);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);
  searchQuery = $state("");
  searchHits = $state<VaultSearchHit[]>([]);
  editorMode = $state<"edit" | "preview">("edit");
  /** Ledger notes: table-first editing (M7c.2). */
  ledgerEditMode = $state<"table" | "raw">("table");
  showSystemNotes = $state(loadShowSystemNotes());
  activeSpaceFilter = $state<string | null>(loadLastSpace());
  newNoteDialogOpen = $state(false);

  private autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  private savedWhisperTimer: ReturnType<typeof setTimeout> | null = null;

  labelByPath(): Map<string, string> {
    return buildVaultLabelMap(this.notes);
  }

  kindByPath(): Map<string, VaultNoteKind> {
    return new Map(
      this.notes.map((note) => [
        note.path,
        resolveKind(note.path, note.kind),
      ]),
    );
  }

  get isDirty(): boolean {
    return this.dirty;
  }

  get lastNotePath(): string | null {
    return loadLastNote();
  }

  get activeSpace(): ReturnType<typeof getSpaceById> {
    if (this.selectedPath) {
      const note = this.notes.find((row) => row.path === this.selectedPath);
      if (note) {
        return resolveSpaceForPath(note.path, note.title);
      }
    }
    if (this.activeSpaceFilter) {
      return getSpaceById(this.activeSpaceFilter);
    }
    return undefined;
  }

  /** Default space for new-note dialog. */
  get defaultCreateSpaceId(): string {
    if (this.activeSpaceFilter && this.activeSpaceFilter !== "system_bucket") {
      return this.activeSpaceFilter;
    }
    const last = loadLastSpace();
    if (last && last !== "system_bucket" && last !== "other") {
      return last;
    }
    return "journal";
  }

  spaceCounts(): Map<string, number> {
    return countNotesBySpace(this.notes, this.showSystemNotes);
  }

  diffStats(): LineDiffStats | null {
    if (!this.dirty) return null;
    return lineDiffStats(this.baselineContent, this.content);
  }

  diffChip(): string | null {
    if (this.saveStatus === "saved") return null;
    const stats = this.diffStats();
    return stats ? formatDiffChip(stats) : null;
  }

  saveWhisper(): string | null {
    if (this.saveStatus === "conflict") return null;
    if (this.saveStatus === "saving") return "Saving…";
    if (this.saveStatus === "saved") return "Saved";
    if (this.dirty || this.saveStatus === "unsaved") return null;
    return null;
  }

  clearAutosaveTimer() {
    if (this.autosaveTimer) {
      clearTimeout(this.autosaveTimer);
      this.autosaveTimer = null;
    }
  }

  clearSavedWhisperTimer() {
    if (this.savedWhisperTimer) {
      clearTimeout(this.savedWhisperTimer);
      this.savedWhisperTimer = null;
    }
  }

  scheduleAutosave() {
    this.clearAutosaveTimer();
    if (!this.selectedPath || !this.dirty || this.saveStatus === "conflict") {
      return;
    }
    this.autosaveTimer = setTimeout(() => {
      void this.save({ source: "autosave" });
    }, VAULT_AUTOSAVE_MS);
  }

  flashSavedWhisper() {
    this.saveStatus = "saved";
    this.clearSavedWhisperTimer();
    this.savedWhisperTimer = setTimeout(() => {
      if (!this.dirty) this.saveStatus = "idle";
    }, VAULT_SAVED_WHISPER_MS);
  }

  resetSaveState() {
    this.clearAutosaveTimer();
    this.clearSavedWhisperTimer();
    this.saveStatus = "idle";
    this.conflictMessage = null;
  }

  rebuildTree() {
    this.tree = buildVaultTree(this.notes, {
      showSystemNotes: this.showSystemNotes,
      spaceFilter: this.activeSpaceFilter,
    });
  }

  setShowSystemNotes(value: boolean) {
    this.showSystemNotes = value;
    saveShowSystemNotes(value);
    this.rebuildTree();
  }

  setActiveSpaceFilter(spaceId: string | null) {
    this.activeSpaceFilter = spaceId;
    saveLastSpace(spaceId);
    this.rebuildTree();
    if (this.searchQuery.trim()) {
      void this.runSearch(this.searchQuery);
    }
  }

  rememberSpaceForPath(path: string, title: string) {
    const space = resolveSpaceForPath(path, title);
    if (space.id === "system_bucket" || space.id === "other") return;
    saveLastSpace(space.id);
  }

  async refreshNotes() {
    this.loading = true;
    this.error = null;
    try {
      const response = await listVaultNotes(undefined, 500);
      this.notes = response.notes;
      this.rebuildTree();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  async openNote(path: string) {
    if (this.dirty && this.selectedPath && this.selectedPath !== path) {
      this.clearAutosaveTimer();
      await this.save({ source: "autosave" });
    }
    this.loading = true;
    this.error = null;
    try {
      const response: VaultNoteContentResponse = await getVaultNote(path);
      this.applyNote(response);
      this.selectedPath = path;
      localStorage.setItem(LAST_NOTE_KEY, path);
      this.rememberSpaceForPath(path, response.note.title);
      await this.refreshBacklinks(path);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  applyNote(response: VaultNoteContentResponse) {
    this.resetSaveState();
    this.content = response.content;
    this.baselineContent = response.content;
    this.contentHash = response.note.content_hash;
    this.title = response.note.title;
    this.selectedKind = resolveKind(response.note.path, response.note.kind);
    this.wikilinksOut = response.note.wikilinks_out;
    this.backlinks = response.note.backlinks;
    this.dirty = false;
    this.editorMode = this.defaultEditorMode(
      response.note.path,
      response.note.kind,
    );
    if (this.selectedKind === "ledger") {
      this.ledgerEditMode = "table";
    }
  }

  defaultEditorMode(path: string, kind?: string): "edit" | "preview" {
    const resolved = resolveKind(path, kind);
    if (resolved === "daily" || resolved === "note") return "preview";
    return "edit";
  }

  setEditorMode(mode: "edit" | "preview") {
    this.editorMode = mode;
  }

  enterEditMode() {
    this.editorMode = "edit";
  }

  enterPreviewMode() {
    this.editorMode = "preview";
  }

  resolveWikilinkPath(rawTarget: string): string | null {
    return resolveWikilinkTarget(rawTarget, this.selectedPath, this.notes);
  }

  openWikilink(rawTarget: string) {
    const path = this.resolveWikilinkPath(rawTarget);
    if (path) void this.openNote(path);
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
    if (this.saveStatus === "conflict") {
      return;
    }
    if (this.saveStatus !== "saving") {
      this.saveStatus = "unsaved";
    }
    this.scheduleAutosave();
  }

  private applySaveResponse(response: VaultNoteContentResponse["note"]) {
    this.contentHash = response.content_hash;
    this.title = response.title;
    this.selectedKind = resolveKind(response.path, response.kind);
    this.wikilinksOut = response.wikilinks_out;
    this.baselineContent = this.content;
    this.dirty = false;
  }

  async save(options?: { force?: boolean; source?: "manual" | "autosave" }) {
    if (!this.selectedPath) return false;
    if (!this.dirty && !options?.force) return true;

    this.clearAutosaveTimer();
    this.saving = true;
    this.saveStatus = "saving";
    this.error = null;

    try {
      const response = await saveVaultNote(
        this.selectedPath,
        this.content,
        options?.force ? undefined : (this.contentHash ?? undefined),
      );
      this.applySaveResponse(response.note);
      this.flashSavedWhisper();
      await this.refreshNotes();
      await this.refreshBacklinks(this.selectedPath);
      return true;
    } catch (err) {
      if (isVaultConflictError(err)) {
        this.saveStatus = "conflict";
        this.conflictMessage =
          "This note changed on disk. Reload the latest version or keep your edits.";
        return false;
      }
      this.saveStatus = "unsaved";
      this.error = err instanceof Error ? err.message : String(err);
      this.scheduleAutosave();
      return false;
    } finally {
      this.saving = false;
    }
  }

  async flushSave() {
    return this.save({ source: "manual" });
  }

  async reloadFromServer() {
    if (!this.selectedPath) return;
    this.loading = true;
    this.error = null;
    try {
      const response = await getVaultNote(this.selectedPath);
      this.applyNote(response);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  async keepMineAndSave() {
    if (!this.selectedPath) return false;
    const ok = await this.save({ force: true, source: "manual" });
    if (ok) {
      this.conflictMessage = null;
    }
    return ok;
  }

  async createNote(options: {
    spaceId: string;
    title: string;
    content?: string;
    path?: string;
    templateId?: VaultTemplateId;
  }) {
    this.saving = true;
    this.error = null;
    try {
      if (options.spaceId !== "system_bucket" && options.spaceId !== "other") {
        this.setActiveSpaceFilter(options.spaceId);
      }
      const space = getSpaceById(options.spaceId);
      const prefix = space?.prefix ?? "";
      const slug = slugifyTitle(options.title);
      const templateId = resolveTemplateForSpace(
        options.spaceId,
        options.templateId,
      );
      const path =
        options.path ??
        pathForTemplate(templateId, options.spaceId, options.title.trim() || slug) ??
        `${prefix}${slug}.md`.replace(/\/+/g, "/").replace(/^\//, "");
      const content =
        options.content ??
        contentForTemplate(
          templateId,
          options.title.trim() || slug,
          new Date(),
          options.spaceId,
        );
      const response = await createVaultNote(path, content);
      await this.refreshNotes();
      await this.openNote(response.note.path);
      return response.note.path;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return null;
    } finally {
      this.saving = false;
    }
  }

  async createWeeklyReview() {
    const path = weeklyReviewPath();
    const existing = this.notes.find((note) => note.path === path);
    if (existing) {
      await this.openNote(path);
      return path;
    }
    return this.createNote({
      spaceId: "journal",
      title: weeklyReviewTitle(),
      path,
      templateId: "weekly",
      content: weeklyReviewTemplate(),
    });
  }

  insertWeeklyReviewLink() {
    if (!this.selectedPath) return;
    const link = weeklyReviewWikilink();
    const plain = link.slice(2, -2);
    if (this.content.includes(plain)) return;
    this.markDirty(insertTextAtSection(this.content, "## Links", link));
  }

  async promoteNote(targetSpaceId: "journal" | "projects") {
    if (!this.selectedPath || this.selectedKind !== "inbox") return null;
    this.saving = true;
    this.error = null;
    const sourcePath = this.selectedPath;
    try {
      const newKind: VaultNoteKind =
        targetSpaceId === "journal" ? "daily" : "project";
      const space = getSpaceById(targetSpaceId);
      const prefix = space?.prefix ?? "";
      const slug = slugifyTitle(this.title || "promoted-note");
      const newPath = `${prefix}${slug}.md`.replace(/\/+/g, "/");
      const promotedContent = setFrontmatterKind(this.content, newKind);
      await createVaultNote(newPath, promotedContent);
      await deleteVaultNote(sourcePath);
      await this.refreshNotes();
      await this.openNote(newPath);
      return newPath;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return null;
    } finally {
      this.saving = false;
    }
  }

  async createDailyNote() {
    const path = dailyNotePath();
    const existing = this.notes.find((note) => note.path === path);
    if (existing) {
      await this.openNote(path);
      return path;
    }
    return this.createNote({
      spaceId: "journal",
      title: `Daily · ${path.replace("journal/", "").replace(".md", "")}`,
      path,
      content: dailyNoteTemplate(),
    });
  }

  async quickCapture(line: string) {
    const trimmed = line.trim();
    if (!trimmed) return null;
    return this.createNote({
      spaceId: "inbox",
      title: "Capture",
      path: inboxCapturePath(),
      content: inboxCaptureTemplate(trimmed),
    });
  }

  async archiveNote(path: string) {
    this.error = null;
    try {
      await deleteVaultNote(path);
      if (this.selectedPath === path) {
        this.selectedPath = null;
        this.content = "";
        this.baselineContent = "";
        this.contentHash = null;
        this.title = "";
        this.selectedKind = "note";
        this.dirty = false;
        this.resetSaveState();
      }
      await this.refreshNotes();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  async runSearch(query: string) {
    this.searchQuery = query;
    if (!query.trim()) {
      this.searchHits = [];
      return;
    }
    try {
      const response = await searchVaultNotes(query.trim(), 20);
      let hits = response.hits;
      if (this.activeSpaceFilter) {
        hits = hits.filter((hit) => {
          const title = hit.note.title;
          return (
            resolveSpaceForPath(hit.note.path, title).id === this.activeSpaceFilter
          );
        });
      }
      if (!this.showSystemNotes) {
        hits = hits.filter(
          (hit) => !isSystemNoiseNote(hit.note.path, hit.note.title),
        );
      }
      this.searchHits = hits.slice(0, 12);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  toggleEditorMode() {
    this.editorMode = this.editorMode === "edit" ? "preview" : "edit";
  }

  toggleLedgerEditMode() {
    this.ledgerEditMode = this.ledgerEditMode === "table" ? "raw" : "table";
  }

  openNewNoteDialog() {
    this.newNoteDialogOpen = true;
  }

  closeNewNoteDialog() {
    this.newNoteDialogOpen = false;
  }
}

function loadLastNote(): string | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage.getItem(LAST_NOTE_KEY);
}

export const vault = new VaultStore();
