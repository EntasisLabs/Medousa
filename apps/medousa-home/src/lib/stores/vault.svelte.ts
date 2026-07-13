import {
  addVaultRoot,
  createVaultNote,
  deleteVaultNote,
  getVaultBacklinks,
  getVaultNote,
  listVaultNotes,
  listVaultRoots,
  listVaultTags,
  saveVaultNote,
  searchVaultNotes,
  setActiveVaultRoot,
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
  VaultRootView,
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
  sortVaultTagsForDisplay,
  type VaultNoteKind,
} from "$lib/utils/vaultFrontmatter";
import { workshopSessionIdForVaultSave } from "$lib/utils/vaultNoteWorkshop";
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
import {
  embedPathForNote,
  formatImageEmbedMarkdown,
} from "$lib/utils/vaultLocalImages";
import { invalidateMedousaViewCache } from "$lib/utils/resolveMedousaViews";
import {
  extractMedousaViewBlocks,
  replaceMedousaViewFenceAt,
  serializeMedousaViewFence,
  type MedousaViewQuery,
} from "$lib/utils/markdownView";
import {
  extractChartFences,
  parseChartFenceParts,
  replaceChartFencePropsAt,
  type ChartFenceKv,
} from "$lib/utils/vaultChartFence";
import { insertTextAtCursor } from "$lib/utils/vaultMarkdownEdit";
import { invalidateTransclusionCache } from "$lib/utils/resolveTransclusion";
import { invalidateVaultRootCache } from "$lib/utils/vaultFilesystem";
import { loadVaultRecent, rememberVaultRecent } from "$lib/utils/vaultRecent";
import {
  formatDiffChip,
  lineDiffStats,
  type LineDiffStats,
} from "$lib/utils/vaultDiff";

const LAST_NOTE_KEY = "medousa-home-last-note";
const LIBRARY_BROWSE_MODE_KEY = "medousa-home-vault-browse-mode";
const EDITOR_SURFACE_KEY = "medousa-home-vault-editor-surface";
const RECENT_BROWSE_LIMIT = 40;
const KIND_BROWSE_ORDER: VaultNoteKind[] = [
  "daily",
  "project",
  "ledger",
  "board",
  "inbox",
  "bug",
  "note",
];

export type VaultProposalSource = "agent" | "operator";
export type LibraryBrowseMode = "folders" | "tags" | "recent" | "kind";

export type VaultTagCount = { tag: string; count: number };

export class VaultStore {
  notes = $state<VaultNote[]>([]);
  tree = $state<VaultTreeNode[]>([]);
  selectedPath = $state<string | null>(loadLastNote());
  content = $state("");
  baselineContent = $state("");
  contentHash = $state<string | null>(null);
  wikilinksOut = $state<string[]>([]);
  backlinks = $state<string[]>([]);
  noteTags = $state<string[]>([]);
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
  /** Write = prose typography; source = mono fence surgery. */
  editorSurface = $state<"write" | "source">(loadEditorSurface());
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
  /** pane = Your files library column; panel = floating popup over a note. */
  previewPresentation = $state<"pane" | "panel">("pane");
  garageWizardOpen = $state(false);
  newGroupDialogOpen = $state(false);
  noteActionsOpen = $state(false);
  vaultRoots = $state<VaultRootView[]>([]);
  activeVaultRootId = $state<string | null>(null);
  vaultRootsLoading = $state(false);
  vaultRootsError = $state<string | null>(null);
  /** Engine lacks GET /v1/vault/roots (older build). */
  vaultRootsUnavailable = $state(false);
  addVaultRootOpen = $state(false);
  recentPaths = $state<string[]>(loadVaultRecent());
  libraryBrowseMode = $state<LibraryBrowseMode>(loadLibraryBrowseMode());
  vaultTags = $state<VaultTagCount[]>([]);
  /** Bumps when note content is replaced externally (open note, reload) — not on typing. */
  contentRevision = $state(0);
  /** Heading fragment from `[[note#Section]]` waiting for preview scroll. */
  pendingHeadingScroll = $state<string | null>(null);
  headingScrollRequest = $state(0);
  newNotePrefillTitle = $state("");
  newNotePrefillPath = $state<string | null>(null);
  pendingEditorInsert = $state<string | null>(null);
  editorInsertRequest = $state(0);
  /** Slash insert or preview configure for medousa-view. */
  viewBridgeOpen = $state(false);
  viewBridgeMode = $state<"insert" | "edit">("insert");
  viewBridgeInsertAt = $state(0);
  viewBridgeEditIndex = $state<number | null>(null);
  viewBridgeQuery = $state<MedousaViewQuery | null>(null);
  chartBridgeOpen = $state(false);
  chartBridgeEditIndex = $state<number | null>(null);
  chartBridgeKv = $state<ChartFenceKv | null>(null);

  private autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  private savedWhisperTimer: ReturnType<typeof setTimeout> | null = null;
  private notesRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  private compositionHold = $state(false);
  private saveEchoPath: string | null = null;
  private saveEchoUntil = 0;

  attachments = $derived(listAttachments(this.content));

  previewingAttachment = $derived.by((): VaultAttachment | null => {
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
  });

  labelByPathMap = $derived(buildVaultLabelMap(this.notes));

  kindByPathMap = $derived(
    new Map(
      this.notes.map((note) => [
        note.path,
        resolveKind(note.path, note.kind),
      ]),
    ),
  );

  contentSyncKey = $derived(`${this.selectedPath ?? ""}:${this.contentRevision}`);

  activeSpace = $derived.by((): ReturnType<typeof getSpaceById> => {
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
  });

  spaceCountsMap = $derived(countNotesBySpace(this.notes, this.showSystemNotes));

  activeVaultRootView = $derived(
    this.vaultRoots.find((root) => root.id === this.activeVaultRootId) ??
      this.vaultRoots.find((root) => root.active) ??
      null,
  );

  diffChipText = $derived.by((): string | null => {
    if (this.saveStatus === "saved") return null;
    if (!this.dirty) return null;
    const stats = lineDiffStats(this.baselineContent, this.content);
    return formatDiffChip(stats);
  });

  get isWriteFirstKind(): boolean {
    return isWriteFirstKind(this.selectedKind);
  }

  labelByPath(): Map<string, string> {
    return this.labelByPathMap;
  }

  kindByPath(): Map<string, VaultNoteKind> {
    return this.kindByPathMap;
  }

  get isDirty(): boolean {
    return this.dirty;
  }

  get lastNotePath(): string | null {
    return loadLastNote();
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
    return this.spaceCountsMap;
  }

  diffStats(): LineDiffStats | null {
    if (!this.dirty) return null;
    return lineDiffStats(this.baselineContent, this.content);
  }

  diffChip(): string | null {
    return this.diffChipText;
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
    // Own autosave/manual save often lands while the next keystroke already marked
    // dirty (kanban debounce). Still ignore the echo — it is not an external edit.
    return (
      event.actor === "operator" &&
      path === this.saveEchoPath &&
      Date.now() < this.saveEchoUntil &&
      path === this.selectedPath
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
    this.rebuildVaultTagsFromNotes();
  }

  setLibraryBrowseMode(mode: LibraryBrowseMode) {
    this.libraryBrowseMode = mode;
    saveLibraryBrowseMode(mode);
    if (mode === "tags") {
      void this.refreshVaultTags();
    }
  }

  /** Notes visible under current space / system / agent-review filters. */
  scopedLibraryNotes(): VaultNote[] {
    const agentMap = this.agentWrittenAt;
    const agentOnly = this.showAgentReviewFilter;
    const showSystem = this.showSystemNotes;
    const spaceFilter = this.activeSpaceFilter;
    return this.notes.filter((note) => {
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
    for (const path of this.recentPaths) {
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

  private rebuildVaultTagsFromNotes(extraTags: string[] = []) {
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
    this.vaultTags = sortVaultTagsForDisplay([...counts.keys()])
      .map((tag) => ({ tag, count: counts.get(tag) ?? 0 }))
      .filter((row) => row.count > 0);
  }

  async refreshVaultTags() {
    try {
      const response = await listVaultTags({ limit: 500 });
      this.rebuildVaultTagsFromNotes(response.tags ?? []);
    } catch {
      this.rebuildVaultTagsFromNotes();
    }
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

      if (serverContent === this.content) {
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

  queueEditorInsert(text: string) {
    this.pendingEditorInsert = text;
    this.editorInsertRequest += 1;
  }

  takeEditorInsert(): string | null {
    const text = this.pendingEditorInsert;
    this.pendingEditorInsert = null;
    return text;
  }

  openViewBridgeInsert(insertAt: number) {
    this.viewBridgeMode = "insert";
    this.viewBridgeInsertAt = insertAt;
    this.viewBridgeEditIndex = null;
    this.viewBridgeQuery = null;
    this.viewBridgeOpen = true;
  }

  openViewBridgeEdit(index: number) {
    const blocks = extractMedousaViewBlocks(this.content);
    const block = blocks[index];
    if (!block) return;
    this.viewBridgeMode = "edit";
    this.viewBridgeEditIndex = index;
    this.viewBridgeQuery = block.query;
    this.viewBridgeOpen = true;
  }

  closeViewBridge() {
    this.viewBridgeOpen = false;
    this.viewBridgeQuery = null;
    this.viewBridgeEditIndex = null;
  }

  commitViewBridge(query: MedousaViewQuery) {
    if (this.viewBridgeMode === "edit" && this.viewBridgeEditIndex != null) {
      const next = replaceMedousaViewFenceAt(
        this.content,
        this.viewBridgeEditIndex,
        query,
      );
      if (next) {
        this.markDirty(next);
        invalidateMedousaViewCache();
      }
    } else {
      const fence = serializeMedousaViewFence(query);
      const result = insertTextAtCursor(
        this.content,
        this.viewBridgeInsertAt,
        fence,
      );
      this.markDirty(result.content);
    }
    this.closeViewBridge();
  }

  openChartBridgeEdit(index: number) {
    const blocks = extractChartFences(this.content);
    const block = blocks[index];
    if (!block) return;
    this.chartBridgeEditIndex = index;
    this.chartBridgeKv = parseChartFenceParts(block.body).kv;
    this.chartBridgeOpen = true;
  }

  closeChartBridge() {
    this.chartBridgeOpen = false;
    this.chartBridgeKv = null;
    this.chartBridgeEditIndex = null;
  }

  commitChartBridge(kv: ChartFenceKv) {
    if (this.chartBridgeEditIndex == null) {
      this.closeChartBridge();
      return;
    }
    const next = replaceChartFencePropsAt(
      this.content,
      this.chartBridgeEditIndex,
      kv,
    );
    if (next) this.markDirty(next);
    this.closeChartBridge();
  }

  async insertImageEmbed(imagePath: string) {
    if (!this.selectedPath || !imagePath.trim()) return;
    this.enterEditMode();
    const embedPath = await embedPathForNote(imagePath, this.selectedPath);
    const alt = embedPath.split("/").pop()?.replace(/\.[^.]+$/, "") ?? "image";
    this.queueEditorInsert(formatImageEmbedMarkdown(embedPath, alt));
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

  get activeVaultRoot(): VaultRootView | null {
    return this.activeVaultRootView;
  }

  resetForWorkshopSwitch() {
    this.clearAutosaveTimer();
    this.clearProposal();
    this.selectedPath = null;
    this.content = "";
    this.baselineContent = "";
    this.contentHash = null;
    this.noteTags = [];
    this.wikilinksOut = [];
    this.backlinks = [];
    this.title = "";
    this.dirty = false;
    this.searchHits = [];
    this.searchQuery = "";
    this.notes = [];
    this.tree = [];
    this.error = null;
    this.vaultRoots = [];
    this.activeVaultRootId = null;
    this.vaultRootsUnavailable = false;
    invalidateVaultRootCache();
    void import("$lib/utils/vaultLocalImages").then(({ clearDaemonImagePreviewCache }) => {
      clearDaemonImagePreviewCache();
    });
    void this.refreshVaultRoots();
    void this.refreshNotes();
  }

  async refreshVaultRoots() {
    this.vaultRootsLoading = true;
    this.vaultRootsError = null;
    try {
      const response = await listVaultRoots();
      this.vaultRootsUnavailable = false;
      this.vaultRoots = response.roots;
      this.activeVaultRootId = response.activeRootId;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (/404|not found/i.test(message)) {
        this.vaultRootsUnavailable = true;
        this.vaultRootsError = null;
        this.vaultRoots = [
          {
            id: "personal",
            label: "Personal",
            path: "",
            isDefault: true,
            active: true,
          },
        ];
        this.activeVaultRootId = "personal";
      } else {
        this.vaultRootsError = message;
      }
    } finally {
      this.vaultRootsLoading = false;
    }
  }

  async switchVaultRoot(rootId: string) {
    if (!rootId.trim() || rootId === this.activeVaultRootId) return;
    this.clearAutosaveTimer();
    this.clearProposal();
    this.selectedPath = null;
    this.content = "";
    this.baselineContent = "";
    this.contentHash = null;
    this.noteTags = [];
    this.wikilinksOut = [];
    this.backlinks = [];
    this.title = "";
    this.dirty = false;
    this.searchHits = [];
    this.searchQuery = "";
    this.notes = [];
    this.tree = [];
    this.error = null;
    invalidateVaultRootCache();
    try {
      const response = await setActiveVaultRoot(rootId);
      this.vaultRoots = response.roots;
      this.activeVaultRootId = response.activeRootId;
      await this.refreshNotes();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  async registerVaultRoot(label: string, path: string) {
    const { isCoLocatedWorkshop, vaultAddRootRemoteHint } = await import(
      "$lib/utils/workshopLocality"
    );
    if (!isCoLocatedWorkshop()) {
      throw new Error(vaultAddRootRemoteHint());
    }
    const response = await addVaultRoot(label, path);
    this.vaultRoots = response.roots;
    this.activeVaultRootId = response.activeRootId;
    invalidateVaultRootCache();
  }

  openAddVaultRootDialog() {
    // Folder pick posts a Home path — only valid when co-located with the daemon.
    void import("$lib/utils/workshopLocality").then(({ isCoLocatedWorkshop }) => {
      if (!isCoLocatedWorkshop()) return;
      this.addVaultRootOpen = true;
    });
  }

  closeAddVaultRootDialog() {
    this.addVaultRootOpen = false;
  }

  async refreshNotes() {
    this.error = null;
    try {
      const response = await listVaultNotes({ limit: 500 });
      this.notes = response.notes;
      this.rebuildTree();
      if (this.libraryBrowseMode === "tags") {
        void this.refreshVaultTags();
      }
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
      this.closeAttachmentPreview();
    }
    this.noteLoading = true;
    this.loading = true;
    this.error = null;
    try {
      const response: VaultNoteContentResponse = await getVaultNote(path);
      this.applyNote(response);
      this.selectedPath = path;
      localStorage.setItem(LAST_NOTE_KEY, path);
      rememberVaultRecent(path);
      this.recentPaths = loadVaultRecent();
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
    this.noteTags = response.note.tags ?? [];
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
    this.noteTags = response.note.tags ?? [];
    this.dirty = false;
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

  setEditorMode(mode: "edit" | "preview") {
    this.editorMode = mode;
  }

  setEditorSurface(surface: "write" | "source") {
    this.editorSurface = surface;
    saveEditorSurface(surface);
  }

  toggleEditorSurface() {
    this.setEditorSurface(this.editorSurface === "write" ? "source" : "write");
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
    this.bumpContentSync();
    if (
      this.previewingAttachmentPath &&
      !listAttachments(nextContent).some(
        (row) => row.path === this.previewingAttachmentPath,
      )
    ) {
      // Keep pane previews from Your files (path may not be in note attachments).
      if (this.previewPresentation === "panel") {
        this.closeAttachmentPreview();
      }
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
    this.noteTags = response.tags ?? [];
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

    const pathSnapshot = this.selectedPath;
    const contentSnapshot = this.content;

    try {
      const response = await saveVaultNote(pathSnapshot, contentSnapshot, {
        contentHash: options?.force ? undefined : (this.contentHash ?? undefined),
        sessionId: workshopSessionIdForVaultSave(pathSnapshot),
      });
      if (this.selectedPath !== pathSnapshot) return true;

      // Typed during in-flight save — keep newer buffer dirty against the snapshot we wrote.
      if (this.content !== contentSnapshot) {
        this.contentHash = response.note.content_hash;
        this.baselineContent = contentSnapshot;
        this.dirty = true;
        this.saveStatus = "unsaved";
        this.markSaveEcho(pathSnapshot);
        this.scheduleNotesRefresh();
        this.scheduleAutosave();
        return true;
      }

      this.applySaveResponse(response.note);
      invalidateMedousaViewCache(pathSnapshot);
      invalidateTransclusionCache(pathSnapshot);
      this.clearProposal();
      this.markSaveEcho(pathSnapshot);
      this.flashSavedWhisper();
      this.scheduleNotesRefresh();
      void this.refreshBacklinks(pathSnapshot);
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
    /** When false, refresh the index but do not open the note (browser save). */
    open?: boolean;
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
      if (options.open !== false) {
        await this.openNote(response.note.path);
      }
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

  setLedgerEditMode(mode: "table" | "raw") {
    this.ledgerEditMode = mode;
  }

  toggleBoardEditMode() {
    this.boardEditMode = this.boardEditMode === "board" ? "raw" : "board";
  }

  setBoardEditMode(mode: "board" | "raw") {
    this.boardEditMode = mode;
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
      this.closeAttachmentPreview();
    }
  }

  previewAttachment(path: string, presentation: "pane" | "panel" = "pane") {
    if (!path.trim()) return;
    this.previewingAttachmentPath = path;
    this.previewPresentation = presentation;
  }

  closeAttachmentPreview() {
    this.previewingAttachmentPath = null;
    this.previewPresentation = "pane";
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

function loadEditorSurface(): "write" | "source" {
  if (typeof localStorage === "undefined") return "write";
  try {
    const raw = localStorage.getItem(EDITOR_SURFACE_KEY);
    if (raw === "source" || raw === "write") return raw;
  } catch {
    /* ignore */
  }
  return "write";
}

function saveEditorSurface(surface: "write" | "source") {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(EDITOR_SURFACE_KEY, surface);
  } catch {
    /* ignore */
  }
}

const LIBRARY_BROWSE_MODES = new Set<LibraryBrowseMode>([
  "folders",
  "tags",
  "recent",
  "kind",
]);

function loadLibraryBrowseMode(): LibraryBrowseMode {
  if (typeof localStorage === "undefined") return "folders";
  try {
    const raw = localStorage.getItem(LIBRARY_BROWSE_MODE_KEY);
    if (raw && LIBRARY_BROWSE_MODES.has(raw as LibraryBrowseMode)) {
      return raw as LibraryBrowseMode;
    }
  } catch {
    /* ignore */
  }
  return "folders";
}

function saveLibraryBrowseMode(mode: LibraryBrowseMode) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(LIBRARY_BROWSE_MODE_KEY, mode);
  } catch {
    /* ignore */
  }
}

const AGENT_WRITE_TTL_MS = 24 * 60 * 60 * 1000;

function isRecentAgentWrite(
  path: string,
  agentWrittenAt: Record<string, string>,
): boolean {
  const writtenAt = agentWrittenAt[path];
  if (!writtenAt) return false;
  return Date.now() - Date.parse(writtenAt) < AGENT_WRITE_TTL_MS;
}

export const vault = new VaultStore();
