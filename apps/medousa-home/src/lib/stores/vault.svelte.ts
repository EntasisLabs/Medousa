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
  shouldHideGarageNote,
  loadLastSpace,
  loadShowSystemNotes,
  resolveSpaceForPath,
  saveLastSpace,
  saveShowSystemNotes,
} from "$lib/config/vaultSpaces";
import {
  readVaultStampCompletionEnabled,
  writeVaultStampCompletionEnabled,
} from "$lib/config/vaultPreferences";
import type { WorkspaceEvent } from "$lib/types/workspace";
import { vaultRefPath } from "$lib/utils/activityEnrichment";
import type {
  VaultNote,
  VaultNoteContentResponse,
  VaultSearchHit,
  VaultTreeNode,
} from "$lib/types/vault";
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
import { parseWikilinkTarget, resolveWikilinkTarget, suggestPathForWikilinkToken } from "$lib/utils/resolveWikilink";
import {
  addAttachments,
  guessMimeFromPath,
  listAttachments,
  removeAttachment as dropAttachment,
  type VaultAttachment,
} from "$lib/utils/vaultAttachments";
import { pickAttachmentFiles, pickSpreadsheetFiles } from "$lib/utils/vaultAttachmentPicker";
import {
  isWriteFirstKind,
  defaultAuthoringMode,
  type VaultAuthoringMode,
} from "$lib/utils/vaultAuthoring";
import {
  isVaultConflictError,
  VAULT_AUTOSAVE_MS,
  VAULT_NOTES_REFRESH_MS,
  VAULT_SAVE_ECHO_MS,
  VAULT_SAVED_WHISPER_MS,
  type VaultSaveStatus,
} from "$lib/utils/vaultSave";
import {
  completeGarageOnboarding,
  shouldShowGarageWizard,
} from "$lib/utils/garageOnboarding";
import { addCustomVaultSpace } from "$lib/utils/vaultCustomSpaces";
import {
  normalizeVaultNotePath,
  setNoteTitleInContent,
} from "$lib/utils/vaultNoteTitle";
import { noteHasKanbanBoard } from "$lib/utils/markdownKanban";
import { togglePreviewTaskInContent } from "$lib/utils/vaultPreviewTasks";
import { invalidateMedousaViewCache } from "$lib/utils/resolveMedousaViews";
import { invalidateTransclusionCache } from "$lib/utils/resolveTransclusion";

const LAST_NOTE_KEY = "medousa-home-last-note";

export type VaultProposalSource = "agent" | "operator";

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
  /** True while fetching note content (open/reload) — not list refresh. */
  noteLoading = $state(false);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);
  searchQuery = $state("");
  searchHits = $state<VaultSearchHit[]>([]);
  editorMode = $state<"edit" | "preview">("edit");
  /** M8b: human write surface vs markdown source (write-first kinds). */
  authoringMode = $state<VaultAuthoringMode>("write");
  /** Ledger notes: table-first editing (M7c.2). */
  ledgerEditMode = $state<"table" | "raw">("table");
  /** Board notes: kanban-first editing (Phase E). */
  boardEditMode = $state<"board" | "raw">("board");
  showSystemNotes = $state(loadShowSystemNotes());
  stampCompletionInline = $state(readVaultStampCompletionEnabled());
  activeSpaceFilter = $state<string | null>(loadLastSpace());
  newNoteDialogOpen = $state(false);
  /** M7f: agent/server edit waiting for accept/discard. */
  proposalActive = $state(false);
  proposalContent = $state<string | null>(null);
  proposalSource = $state<VaultProposalSource>("agent");
  showAgentReviewFilter = $state(false);
  agentWrittenAt = $state<Record<string, string>>({});
  previewingAttachmentPath = $state<string | null>(null);
  garageWizardOpen = $state(false);
  newGroupDialogOpen = $state(false);
  noteActionsOpen = $state(false);
  /** Bumps when note content is replaced externally (open note, reload) — not on typing. */
  contentRevision = $state(0);
  /** Heading fragment from `[[note#Section]]` waiting for preview scroll. */
  pendingHeadingScroll = $state<string | null>(null);
  headingScrollRequest = $state(0);
  newNotePrefillTitle = $state("");
  newNotePrefillPath = $state<string | null>(null);

  private autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  private savedWhisperTimer: ReturnType<typeof setTimeout> | null = null;
  private notesRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  private compositionHold = $state(false);
  private saveEchoPath: string | null = null;
  private saveEchoUntil = 0;

  get isWriteFirstKind(): boolean {
    return isWriteFirstKind(this.selectedKind);
  }

  get isAuthoringSource(): boolean {
    return this.authoringMode === "source";
  }

  get attachments(): VaultAttachment[] {
    return listAttachments(this.content);
  }

  get previewingAttachment(): VaultAttachment | null {
    if (!this.previewingAttachmentPath) return null;
    const attached = this.attachments.find(
      (row) => row.path === this.previewingAttachmentPath,
    );
    if (attached) return attached;
    const path = this.previewingAttachmentPath;
    const name = path.split(/[/\\]/).pop() ?? path;
    return {
      path,
      label: name,
      mime: guessMimeFromPath(path),
    };
  }

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

  get contentSyncKey(): string {
    return `${this.selectedPath ?? ""}:${this.contentRevision}`;
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
    if (
      !this.selectedPath ||
      !this.dirty ||
      this.saveStatus === "conflict" ||
      this.proposalActive ||
      this.compositionHold
    ) {
      return;
    }
    this.autosaveTimer = setTimeout(() => {
      void this.save({ source: "autosave" });
    }, VAULT_AUTOSAVE_MS);
  }

  /** Pause autosave while slash menu or similar editor UI is active. */
  setCompositionHold(active: boolean) {
    if (this.compositionHold === active) return;
    this.compositionHold = active;
    if (active) {
      this.clearAutosaveTimer();
      return;
    }
    if (this.dirty) {
      this.scheduleAutosave();
    }
  }

  scheduleNotesRefresh() {
    if (this.notesRefreshTimer) {
      clearTimeout(this.notesRefreshTimer);
    }
    this.notesRefreshTimer = setTimeout(() => {
      this.notesRefreshTimer = null;
      void this.refreshNotes();
    }, VAULT_NOTES_REFRESH_MS);
  }

  private markSaveEcho(path: string) {
    this.saveEchoPath = path;
    this.saveEchoUntil = Date.now() + VAULT_SAVE_ECHO_MS;
  }

  private shouldIgnoreSaveEcho(event: WorkspaceEvent, path: string): boolean {
    return (
      event.actor === "operator" &&
      path === this.saveEchoPath &&
      Date.now() < this.saveEchoUntil &&
      path === this.selectedPath &&
      !this.dirty
    );
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

  private bumpContentSync() {
    this.contentRevision += 1;
  }

  rebuildTree() {
    this.tree = buildVaultTree(this.notes, {
      showSystemNotes: this.showSystemNotes,
      spaceFilter: this.activeSpaceFilter,
      agentReviewOnly: this.showAgentReviewFilter,
      agentWrittenAt: this.agentWrittenAt,
    });
  }

  setShowAgentReviewFilter(value: boolean) {
    this.showAgentReviewFilter = value;
    this.rebuildTree();
  }

  private clearProposal() {
    this.proposalActive = false;
    this.proposalContent = null;
  }

  proposalDiffStats(): LineDiffStats | null {
    if (!this.proposalContent) return null;
    return lineDiffStats(this.content, this.proposalContent);
  }

  private recordAgentWrite(path: string, timestampUtc?: string) {
    this.agentWrittenAt = {
      ...this.agentWrittenAt,
      [path]: timestampUtc ?? new Date().toISOString(),
    };
  }

  noteFromFeedEvent(event: WorkspaceEvent) {
    const path = vaultRefPath(event);
    if (!path) return;
    if (event.actor === "agent") {
      this.recordAgentWrite(path, event.timestamp_utc);
    }
    if (this.shouldIgnoreSaveEcho(event, path)) {
      this.scheduleNotesRefresh();
      return;
    }
    void this.ingestRemoteUpdate(event);
    this.scheduleNotesRefresh();
  }

  async ingestRemoteUpdate(event: WorkspaceEvent) {
    const path = vaultRefPath(event);
    if (!path || path !== this.selectedPath) return;

    this.clearAutosaveTimer();
    try {
      const response = await getVaultNote(path);
      const serverContent = response.content;
      const isAgent = event.actor === "agent";

      if (serverContent === this.content && !this.dirty) {
        this.syncNoteMetadata(response);
        return;
      }

      if (isAgent || this.dirty || this.proposalActive) {
        this.proposalActive = true;
        this.proposalContent = serverContent;
        this.proposalSource = isAgent ? "agent" : "operator";
        this.contentHash = response.note.content_hash;
        this.title = response.note.title;
        this.wikilinksOut = response.note.wikilinks_out;
        this.backlinks = response.note.backlinks;
        if (this.dirty && this.saveStatus !== "conflict") {
          this.saveStatus = "unsaved";
        }
        return;
      }

      this.applyNote(response);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  async acceptProposal() {
    if (!this.selectedPath || !this.proposalActive) return false;
    this.clearProposal();
    return this.save({ force: true, source: "manual" });
  }

  async discardProposal() {
    if (!this.selectedPath || !this.proposalContent) return;
    this.clearAutosaveTimer();
    this.content = this.proposalContent;
    this.baselineContent = this.proposalContent;
    this.dirty = false;
    this.resetSaveState();
    this.clearProposal();
    await this.reloadFromServer();
  }

  editProposal() {
    this.proposalActive = false;
  }

  setShowSystemNotes(value: boolean) {
    this.showSystemNotes = value;
    saveShowSystemNotes(value);
    this.rebuildTree();
  }

  setStampCompletionInline(value: boolean) {
    this.stampCompletionInline = value;
    writeVaultStampCompletionEnabled(value);
  }

  togglePreviewTask(taskIndex: number, checked: boolean) {
    if (!this.selectedPath || this.proposalActive) return;
    const next = togglePreviewTaskInContent(
      this.content,
      taskIndex,
      checked,
      this.stampCompletionInline,
    );
    if (!next || next === this.content) return;
    this.markDirty(next);
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

  /** After a move, show the destination space in the sidebar filter. */
  focusSpaceForPath(path: string, title: string) {
    const space = resolveSpaceForPath(path, title);
    if (space.id === "system_bucket" || space.id === "other") {
      this.setActiveSpaceFilter(null);
      return;
    }
    this.setActiveSpaceFilter(space.id);
    saveLastSpace(space.id);
  }

  /** When sidebar filter is All, keep All after drag-move; otherwise focus destination space. */
  applySpaceFilterAfterMove(path: string, title: string, filterWasAll: boolean) {
    if (filterWasAll) return;
    this.focusSpaceForPath(path, title);
  }

  async refreshNotes() {
    this.error = null;
    try {
      const response = await listVaultNotes(undefined, 500);
      this.notes = response.notes;
      this.rebuildTree();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  async openNote(path: string) {
    if (this.dirty && this.selectedPath && this.selectedPath !== path) {
      this.clearAutosaveTimer();
      await this.save({ source: "autosave" });
    }
    if (this.selectedPath !== path) {
      this.clearProposal();
      this.previewingAttachmentPath = null;
    }
    this.noteLoading = true;
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
      this.noteLoading = false;
      this.loading = false;
      if (this.pendingHeadingScroll) {
        this.headingScrollRequest += 1;
      }
    }
  }

  private syncNoteMetadata(response: VaultNoteContentResponse) {
    this.contentHash = response.note.content_hash;
    this.title = response.note.title;
    this.selectedKind = resolveKind(response.note.path, response.note.kind);
    this.wikilinksOut = response.note.wikilinks_out;
    this.backlinks = response.note.backlinks;
  }

  applyNote(
    response: VaultNoteContentResponse,
    options?: { preserveProposal?: boolean },
  ) {
    if (!options?.preserveProposal) {
      this.clearProposal();
    }
    this.resetSaveState();
    this.content = response.content;
    this.baselineContent = response.content;
    this.contentHash = response.note.content_hash;
    this.title = response.note.title;
    this.selectedKind = resolveKind(response.note.path, response.note.kind);
    this.wikilinksOut = response.note.wikilinks_out;
    this.backlinks = response.note.backlinks;
    this.dirty = false;
    this.authoringMode = defaultAuthoringMode(this.selectedKind);
    this.editorMode = "edit";
    if (this.selectedKind === "ledger") {
      this.ledgerEditMode = "table";
    }
    if (noteHasKanbanBoard(response.content) || this.selectedKind === "board") {
      this.boardEditMode = "board";
    }
    this.bumpContentSync();
  }

  defaultEditorMode(path: string, kind?: string): "edit" | "preview" {
    const resolved = resolveKind(path, kind);
    if (isWriteFirstKind(resolved)) return "edit";
    return "edit";
  }

  setAuthoringMode(mode: VaultAuthoringMode) {
    this.authoringMode = mode;
    if (mode === "write") {
      this.editorMode = "edit";
    }
  }

  toggleAuthoringMode() {
    this.setAuthoringMode(this.authoringMode === "write" ? "source" : "write");
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
    const decoded = decodeURIComponent(rawTarget.trim());
    const { pathToken, heading } = parseWikilinkTarget(decoded);
    const path = resolveWikilinkTarget(
      pathToken || decoded,
      this.selectedPath,
      this.notes,
    );
    if (!path) {
      this.openNewNoteDialogForWikilink(decoded);
      return;
    }

    if (heading) {
      this.pendingHeadingScroll = heading;
    }

    if (path === this.selectedPath) {
      if (heading) {
        this.headingScrollRequest += 1;
      }
      this.enterPreviewMode();
      return;
    }

    void this.openNote(path).then(() => {
      if (heading) {
        this.enterPreviewMode();
      }
    });
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
    if (
      this.previewingAttachmentPath &&
      !listAttachments(nextContent).some(
        (row) => row.path === this.previewingAttachmentPath,
      )
    ) {
      this.previewingAttachmentPath = null;
    }
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
    if (this.proposalActive && !options?.force) return false;

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
      invalidateMedousaViewCache(this.selectedPath);
      invalidateTransclusionCache(this.selectedPath);
      this.clearProposal();
      this.markSaveEcho(this.selectedPath);
      this.flashSavedWhisper();
      this.scheduleNotesRefresh();
      void this.refreshBacklinks(this.selectedPath);
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
    this.noteLoading = true;
    this.error = null;
    try {
      const response = await getVaultNote(this.selectedPath);
      this.applyNote(response);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.noteLoading = false;
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
        this.bumpContentSync();
      }
      await this.refreshNotes();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  openNewGroupDialog() {
    this.newGroupDialogOpen = true;
  }

  closeNewGroupDialog() {
    this.newGroupDialogOpen = false;
  }

  openNoteActions() {
    this.noteActionsOpen = true;
  }

  closeNoteActions() {
    this.noteActionsOpen = false;
  }

  async openNoteActionsForPath(path: string) {
    if (this.selectedPath !== path) {
      await this.openNote(path);
    }
    this.openNoteActions();
  }

  private suggestDuplicatePath(sourcePath: string): string {
    const parts = sourcePath.split("/").filter(Boolean);
    const file = parts.pop() ?? "note.md";
    const dir = parts.length ? `${parts.join("/")}/` : "";
    const stem = file.replace(/\.md$/i, "") || "note";
    for (let n = 1; n < 50; n += 1) {
      const suffix = n === 1 ? "-copy" : `-copy-${n}`;
      const candidate = normalizeVaultNotePath(`${dir}${stem}${suffix}.md`);
      if (!this.notes.some((note) => note.path === candidate)) {
        return candidate;
      }
    }
    return normalizeVaultNotePath(`${dir}${stem}-copy-${Date.now()}.md`);
  }

  async duplicateNote(sourcePath: string): Promise<string | null> {
    this.error = null;
    const newPath = this.suggestDuplicatePath(sourcePath);
    try {
      const response = await getVaultNote(sourcePath);
      await createVaultNote(newPath, response.content);
      await this.refreshNotes();
      await this.openNote(newPath);
      return newPath;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return null;
    }
  }

  async copyNoteMarkdown(path: string): Promise<string | null> {
    try {
      if (this.selectedPath === path && this.content) {
        return this.content;
      }
      const response = await getVaultNote(path);
      return response.content;
    } catch {
      return null;
    }
  }

  addCustomGroup(label: string) {
    const space = addCustomVaultSpace(label);
    if (space) {
      this.rebuildTree();
      this.setActiveSpaceFilter(space.id);
    }
    return space;
  }

  async renameNoteTitle(newTitle: string) {
    if (!this.selectedPath || !newTitle.trim()) return false;
    this.error = null;
    const nextContent = setNoteTitleInContent(this.content, newTitle.trim());
    this.markDirty(nextContent);
    const ok = await this.save({ source: "manual" });
    if (ok) {
      await this.refreshNotes();
    }
    return ok;
  }

  async relocateNote(newPathInput: string) {
    if (!this.selectedPath) return null;
    const sourcePath = this.selectedPath;
    const newPath = normalizeVaultNotePath(newPathInput);
    if (newPath === sourcePath) return newPath;

    if (this.notes.some((note) => note.path === newPath)) {
      this.error = "A note already exists at that path.";
      return null;
    }

    this.saving = true;
    this.error = null;
    const filterWasAll = this.activeSpaceFilter === null;
    try {
      if (this.dirty) {
        const saved = await this.save({ source: "manual" });
        if (!saved) return null;
      }
      const response = await getVaultNote(sourcePath);
      await createVaultNote(newPath, response.content);
      await deleteVaultNote(sourcePath);
      await this.refreshNotes();
      this.applySpaceFilterAfterMove(newPath, response.note.title, filterWasAll);
      await this.openNote(newPath);
      return newPath;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return null;
    } finally {
      this.saving = false;
    }
  }

  async moveNoteToFolder(sourcePath: string, targetFolderPrefix: string) {
    let prefix = targetFolderPrefix.trim().replace(/\\/g, "/");
    if (!prefix) {
      this.error = "Pick a folder to move this note into.";
      return null;
    }
    if (!prefix.endsWith("/")) {
      prefix = `${prefix}/`;
    }

    const fileName = sourcePath.split("/").pop();
    if (!fileName) return null;
    const newPath = `${prefix}${fileName}`.replace(/\/+/g, "/");
    if (newPath === sourcePath) return sourcePath;

    if (this.selectedPath === sourcePath) {
      return this.relocateNote(newPath);
    }

    this.saving = true;
    this.error = null;
    const filterWasAll = this.activeSpaceFilter === null;
    try {
      if (this.notes.some((note) => note.path === newPath)) {
        this.error = "A note already exists at that path.";
        return null;
      }
      const response = await getVaultNote(sourcePath);
      await createVaultNote(newPath, response.content);
      await deleteVaultNote(sourcePath);
      await this.refreshNotes();
      this.applySpaceFilterAfterMove(newPath, response.note.title, filterWasAll);
      if (this.selectedPath === sourcePath) {
        await this.openNote(newPath);
      }
      return newPath;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return null;
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
          (hit) => !shouldHideGarageNote(hit.note.path, hit.note.title, this.showSystemNotes),
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

  toggleBoardEditMode() {
    this.boardEditMode = this.boardEditMode === "board" ? "raw" : "board";
  }

  async linkAttachmentFiles() {
    if (!this.selectedPath) return;
    const picked = await pickAttachmentFiles();
    if (picked.length === 0) return;
    this.markDirty(addAttachments(this.content, picked));
  }

  async linkSpreadsheetFiles() {
    if (!this.selectedPath) return;
    const picked = await pickSpreadsheetFiles();
    if (picked.length === 0) return;
    this.markDirty(addAttachments(this.content, picked));
  }

  linkExternalFile(path: string) {
    if (!this.selectedPath) return false;
    const name = path.split(/[/\\]/).pop() ?? path;
    this.markDirty(
      addAttachments(this.content, [
        {
          path,
          label: name,
          mime: guessMimeFromPath(path),
        },
      ]),
    );
    return true;
  }

  removeAttachment(path: string) {
    if (!this.selectedPath) return;
    this.markDirty(dropAttachment(this.content, path));
    if (this.previewingAttachmentPath === path) {
      this.previewingAttachmentPath = null;
    }
  }

  previewAttachment(path: string) {
    if (!path.trim()) return;
    this.previewingAttachmentPath = path;
  }

  closeAttachmentPreview() {
    this.previewingAttachmentPath = null;
  }

  openGarageWizard() {
    this.garageWizardOpen = true;
  }

  closeGarageWizard() {
    this.garageWizardOpen = false;
  }

  finishGarageOnboarding() {
    completeGarageOnboarding();
    this.garageWizardOpen = false;
  }

  shouldPromptGarageWizard(): boolean {
    return shouldShowGarageWizard();
  }

  openNewNoteDialog() {
    this.newNotePrefillTitle = "";
    this.newNoteDialogOpen = true;
  }

  openNewNoteDialogForWikilink(rawTarget: string) {
    const { pathToken } = parseWikilinkTarget(rawTarget);
    const token = pathToken || rawTarget.trim();
    const stem = token.split("/").pop()?.replace(/\.md$/i, "") ?? token;
    this.newNotePrefillTitle = stem.replace(/[-_]+/g, " ");
    this.newNotePrefillPath = suggestPathForWikilinkToken(rawTarget, this.selectedPath);
    this.newNoteDialogOpen = true;
  }

  closeNewNoteDialog() {
    this.newNoteDialogOpen = false;
    this.newNotePrefillTitle = "";
    this.newNotePrefillPath = null;
  }
}

function loadLastNote(): string | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage.getItem(LAST_NOTE_KEY);
}

export const vault = new VaultStore();
