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
  readVaultBuildAutoSave,
  readVaultBuildLineNumbers,
  readVaultBuildScrollSync,
  readVaultBuildWordWrap,
  readVaultStampCompletionEnabled,
  cycleVaultReadingPalette,
  readVaultReadingPalette,
  writeVaultBuildAutoSave,
  writeVaultBuildLineNumbers,
  writeVaultBuildScrollSync,
  writeVaultBuildWordWrap,
  writeVaultReadingPalette,
  writeVaultStampCompletionEnabled,
  type VaultReadingPalette,
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
  normalizeKind,
  parseFrontmatterTitle,
  resolveKind,
  setFrontmatterKind,
  sortVaultTagsForDisplay,
  stripFrontmatter,
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
import { noteHasSlidesDeck } from "$lib/utils/markdownSlides";
import { togglePreviewTaskInContent } from "$lib/utils/vaultPreviewTasks";
import {
  embedPathForNote,
  formatImageEmbedMarkdown,
} from "$lib/utils/vaultLocalImages";
import { invalidateMedousaViewCache } from "$lib/utils/resolveMedousaViews";
import { type NoteBuffer } from "$lib/stores/noteBuffer";
import {
  NoteSaveQueue,
  type NoteSaveJob,
  type NoteSaveResult,
} from "$lib/stores/noteSaveQueue";
import { noteEditorRuntimes } from "$lib/stores/noteEditorRuntimes.svelte";
import { invokeVaultLeaveFlush } from "$lib/stores/vaultLeaveFlush";
import { significantLiveText } from "$lib/vault/live/liveMarkdownCodec";
import {
  extractMedousaViewBlocks,
  replaceMedousaViewFenceAt,
  serializeMedousaViewFence,
  type MedousaViewQuery,
} from "$lib/utils/markdownView";
import {
  extractChartFences,
  parseChartFenceParts,
  replaceChartFenceAt,
  type ChartFenceKv,
} from "$lib/utils/vaultChartFence";
import {
  extractLiquidFences,
  parseLiquidFenceDraft,
  replaceLiquidFenceRawAt,
  serializeLiquidFenceDraft,
  type LiquidFenceDraft,
  type LiquidFenceLang,
} from "$lib/utils/vaultLiquidFence";
import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";
import { insertTextAtCursor } from "$lib/utils/vaultMarkdownEdit";
import { invalidateTransclusionCache } from "$lib/utils/resolveTransclusion";
import {
  fileNameFromAbsolutePath,
  invalidateVaultRootCache,
  pickMarkdownFile,
  readAbsoluteTextFile,
  writeAbsoluteTextFile,
} from "$lib/utils/vaultFilesystem";
import { loadVaultRecent, rememberVaultRecent } from "$lib/utils/vaultRecent";
import {
  formatDiffChip,
  lineDiffStats,
  type LineDiffStats,
} from "$lib/utils/vaultDiff";

const LAST_NOTE_KEY = "medousa-home-last-note";
const LIBRARY_BROWSE_MODE_KEY = "medousa-home-vault-browse-mode";
const EDITOR_SURFACE_KEY = "medousa-home-vault-editor-surface";
/** LME writing plane: Live = calm page; Build = full chrome. */
const NOTE_PLANE_KEY = "medousa-home-vault-note-plane";

export type VaultNotePlane = "live" | "build";
const RECENT_BROWSE_LIMIT = 40;
const KIND_BROWSE_ORDER: VaultNoteKind[] = [
  "daily",
  "project",
  "ledger",
  "board",
  "slides",
  "resume",
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
  /** Absolute path when editing a single .md outside any vault root. */
  looseFilePath = $state<string | null>(null);
  content = $state("");
  baselineContent = $state("");
  contentHash = $state<string | null>(null);
  /**
   * Background pane note snapshots (path-keyed). Focused note stays on public fields.
   * Bumps `noteBufferRevision` when a non-focused buffer changes.
   */
  private noteBuffers = new Map<string, NoteBuffer>();
  noteBufferRevision = $state(0);
  /** Per-path serialized PUTs — coalesce superseding bodies; never reuse stale If-Match. */
  private saveQueue = new NoteSaveQueue((path, job) => this.runSaveJob(path, job));
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
  /** Live = calm LME page; Build = format bar / split / source depth. */
  notePlane = $state<VaultNotePlane>(loadNotePlane());
  /** Ledger notes: table-first editing (M7c.2). */
  ledgerEditMode = $state<"table" | "raw">("table");
  /** Board notes: kanban-first editing (Phase E). */
  boardEditMode = $state<"board" | "raw">("board");
  /** Slides notes: deck-first editing. */
  deckEditMode = $state<"deck" | "raw">("deck");
  showSystemNotes = $state(loadShowSystemNotes());
  stampCompletionInline = $state(readVaultStampCompletionEnabled());
  /** Build editor: wrap long lines (CodeMirror). */
  buildWordWrap = $state(readVaultBuildWordWrap());
  /** Build editor: show line numbers gutter. */
  buildLineNumbers = $state(readVaultBuildLineNumbers());
  /** Autosave dirty notes on a timer. */
  buildAutoSave = $state(readVaultBuildAutoSave());
  /** Build split: sync CodeMirror ↔ Preview scroll. */
  buildScrollSync = $state(readVaultBuildScrollSync());
  /** Live / preview reading palette (Medousa-native, not shell theme). */
  readingPalette = $state<VaultReadingPalette>(readVaultReadingPalette());
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
  chartBridgeTableMarkdown = $state("");
  liquidBridgeOpen = $state(false);
  liquidBridgeLang = $state<LiquidFenceLang | null>(null);
  liquidBridgeEditIndex = $state<number | null>(null);
  liquidBridgeDraft = $state<LiquidFenceDraft | null>(null);
  /** Vault card detail sheet (same payload as chat Liquid cards). */
  cardDetailOpen = $state(false);
  cardDetail = $state<CardDetailPayload | null>(null);

  private autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  private savedWhisperTimer: ReturnType<typeof setTimeout> | null = null;
  private notesRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  private compositionHold = $state(false);
  private saveEchoPath: string | null = null;
  private saveEchoUntil = 0;
  /**
   * Bumped on every openNote / openLooseFile start. Stale fetch completions and
   * remount emits must not apply when this no longer matches their generation.
   */
  private openGeneration = 0;

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

  contentSyncKey = $derived(
    `${this.looseFilePath ?? this.selectedPath ?? ""}:${this.contentRevision}`,
  );

  get isLooseFile(): boolean {
    return Boolean(this.looseFilePath);
  }

  activeSpace = $derived.by((): ReturnType<typeof getSpaceById> => {
    if (this.looseFilePath) return undefined;
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
      !this.buildAutoSave ||
      !this.selectedPath ||
      !this.dirty ||
      this.noteLoading ||
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

  private bumpNoteBuffers() {
    this.noteBufferRevision += 1;
  }

  private normalizeNotePath(path: string): string {
    return normalizeVaultNotePath(path.trim()) || path.trim();
  }

  isFocusedPath(path: string | null | undefined): boolean {
    const trimmed = path?.trim();
    if (!trimmed) return false;
    // Loose markdown uses an absolute OS path — do not vault-normalize (strips drive/root).
    if (this.looseFilePath) {
      return trimmed === this.looseFilePath || trimmed === (this.selectedPath?.trim() ?? "");
    }
    return this.normalizeNotePath(trimmed) === (this.selectedPath?.trim() ?? "");
  }

  /** Markdown for any path — focused live fields or a background buffer. */
  contentFor(path: string): string {
    void this.noteBufferRevision;
    void this.content;
    const trimmed = this.normalizeNotePath(path);
    if (!trimmed) return "";
    if (this.isFocusedPath(trimmed)) return this.content;
    return this.noteBuffers.get(trimmed)?.content ?? "";
  }

  contentSyncKeyFor(path: string): string {
    void this.noteBufferRevision;
    void this.contentSyncKey;
    const trimmed = this.normalizeNotePath(path);
    if (!trimmed) return "";
    if (this.isFocusedPath(trimmed)) return this.contentSyncKey;
    const buffer = this.noteBuffers.get(trimmed);
    return `${trimmed}:${buffer?.contentRevision ?? 0}`;
  }

  titleFor(path: string): string {
    void this.noteBufferRevision;
    void this.title;
    const trimmed = this.normalizeNotePath(path);
    if (!trimmed) return "";
    if (this.isFocusedPath(trimmed)) return this.title;
    return this.noteBuffers.get(trimmed)?.title ?? "";
  }

  noteLoadingFor(path: string): boolean {
    void this.noteBufferRevision;
    void this.noteLoading;
    const trimmed = this.normalizeNotePath(path);
    if (!trimmed) return false;
    if (this.isFocusedPath(trimmed)) return this.noteLoading;
    const buffer = this.noteBuffers.get(trimmed);
    return !buffer && this.bufferWarmInFlight.has(trimmed);
  }

  private bufferWarmInFlight = new Set<string>();

  /** Test helper: seed a background buffer without network. */
  seedBufferForTest(buffer: NoteBuffer) {
    const key = this.normalizeNotePath(buffer.path);
    this.noteBuffers.set(key, { ...buffer, path: key });
    this.bumpNoteBuffers();
  }

  private stashSelectedBuffer() {
    const path = this.selectedPath?.trim();
    if (!path || this.isLooseFile) return;
    const key = this.normalizeNotePath(path);
    this.noteBuffers.set(key, {
      path: key,
      content: this.content,
      baselineContent: this.baselineContent,
      contentHash: this.contentHash,
      title: this.title,
      dirty: this.dirty,
      contentRevision: this.contentRevision,
    });
    this.bumpNoteBuffers();
  }

  private writeBufferFromResponse(path: string, response: VaultNoteContentResponse) {
    const key = this.normalizeNotePath(path);
    this.noteBuffers.set(key, {
      path: key,
      content: response.content,
      baselineContent: response.content,
      contentHash: response.note.content_hash,
      title: response.note.title,
      dirty: false,
      contentRevision: (this.noteBuffers.get(key)?.contentRevision ?? 0) + 1,
    });
    this.bumpNoteBuffers();
  }

  /** Prefetch a note into a background buffer (multi-pane Workspace). */
  async warmBuffer(path: string) {
    const trimmed = this.normalizeNotePath(path);
    if (!trimmed || this.isFocusedPath(trimmed)) return;
    if (this.noteBuffers.has(trimmed) || this.bufferWarmInFlight.has(trimmed)) {
      return;
    }
    this.bufferWarmInFlight.add(trimmed);
    this.bumpNoteBuffers();
    try {
      const response = await getVaultNote(trimmed);
      if (this.isFocusedPath(trimmed)) return;
      this.writeBufferFromResponse(trimmed, response);
    } catch {
      // Leave pane empty; focused openNote will surface errors.
    } finally {
      this.bufferWarmInFlight.delete(trimmed);
      this.bumpNoteBuffers();
    }
  }

  private restoreBufferIntoFocused(buffer: NoteBuffer) {
    this.content = buffer.content;
    this.baselineContent = buffer.baselineContent;
    this.contentHash = buffer.contentHash;
    this.title = buffer.title;
    this.dirty = buffer.dirty;
    this.contentRevision = buffer.contentRevision;
    this.selectedKind = resolveKind(buffer.path, undefined);
    if (noteHasKanbanBoard(buffer.content) || this.selectedKind === "board") {
      this.boardEditMode = "board";
    }
    if (noteHasSlidesDeck(buffer.content) || this.selectedKind === "slides") {
      this.deckEditMode = "deck";
    }
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
    if (this.noteLoading) return;

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

  setBuildWordWrap(value: boolean) {
    this.buildWordWrap = value;
    writeVaultBuildWordWrap(value);
  }

  setBuildLineNumbers(value: boolean) {
    this.buildLineNumbers = value;
    writeVaultBuildLineNumbers(value);
  }

  setBuildAutoSave(value: boolean) {
    this.buildAutoSave = value;
    writeVaultBuildAutoSave(value);
    if (value) {
      if (this.dirty) this.scheduleAutosave();
    } else {
      this.clearAutosaveTimer();
    }
  }

  setBuildScrollSync(value: boolean) {
    this.buildScrollSync = value;
    writeVaultBuildScrollSync(value);
  }

  setReadingPalette(palette: VaultReadingPalette) {
    this.readingPalette = palette;
    writeVaultReadingPalette(palette);
  }

  cycleReadingPalette() {
    this.setReadingPalette(cycleVaultReadingPalette(this.readingPalette));
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
    this.markDirty(next, { reloadEditors: true });
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
        this.markDirty(next, { reloadEditors: true });
        invalidateMedousaViewCache();
      }
    } else {
      const fence = serializeMedousaViewFence(query);
      const result = insertTextAtCursor(
        this.content,
        this.viewBridgeInsertAt,
        fence,
      );
      this.markDirty(result.content, { reloadEditors: true });
    }
    this.closeViewBridge();
  }

  openChartBridgeEdit(index: number) {
    const blocks = extractChartFences(this.content);
    const block = blocks[index];
    if (!block) return;
    const parts = parseChartFenceParts(block.body);
    this.chartBridgeEditIndex = index;
    this.chartBridgeKv = parts.kv;
    this.chartBridgeTableMarkdown = parts.tableMarkdown;
    this.chartBridgeOpen = true;
  }

  closeChartBridge() {
    this.chartBridgeOpen = false;
    this.chartBridgeKv = null;
    this.chartBridgeTableMarkdown = "";
    this.chartBridgeEditIndex = null;
  }

  commitChartBridge(kv: ChartFenceKv, tableMarkdown?: string) {
    if (this.chartBridgeEditIndex == null) {
      this.closeChartBridge();
      return;
    }
    const next = replaceChartFenceAt(
      this.content,
      this.chartBridgeEditIndex,
      kv,
      tableMarkdown,
    );
    if (next) this.markDirty(next, { reloadEditors: true });
    this.closeChartBridge();
  }

  openLiquidBridgeEdit(lang: LiquidFenceLang, index: number) {
    const blocks = extractLiquidFences(this.content, lang);
    const block = blocks[index];
    if (!block) return;
    this.liquidBridgeLang = lang;
    this.liquidBridgeEditIndex = index;
    this.liquidBridgeDraft = parseLiquidFenceDraft(lang, block.body);
    this.liquidBridgeOpen = true;
  }

  closeLiquidBridge() {
    this.liquidBridgeOpen = false;
    this.liquidBridgeLang = null;
    this.liquidBridgeEditIndex = null;
    this.liquidBridgeDraft = null;
  }

  commitLiquidBridge(next: LiquidFenceDraft) {
    if (this.liquidBridgeEditIndex == null || !this.liquidBridgeLang) {
      this.closeLiquidBridge();
      return;
    }
    const raw = serializeLiquidFenceDraft(next);
    const replaced = replaceLiquidFenceRawAt(
      this.content,
      this.liquidBridgeLang,
      this.liquidBridgeEditIndex,
      raw,
    );
    if (replaced) this.markDirty(replaced, { reloadEditors: true });
    this.closeLiquidBridge();
  }

  openCardDetail(detail: CardDetailPayload) {
    this.cardDetail = detail;
    this.cardDetailOpen = true;
  }

  closeCardDetail() {
    this.cardDetailOpen = false;
    this.cardDetail = null;
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
    this.clearLooseFile();
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

  clearLooseFile() {
    this.looseFilePath = null;
  }

  /** Open a single .md file without registering a vault root (desktop, co-located). */
  async openLooseMarkdownFile() {
    const path = await pickMarkdownFile();
    if (!path) return false;
    return this.openLooseFile(path);
  }

  async openLooseFile(absolutePath: string) {
    const trimmed = absolutePath.trim();
    if (!trimmed) return false;
    if (this.dirty && this.selectedPath) {
      this.clearAutosaveTimer();
      await this.save({ source: "autosave" });
    }
    const openGen = ++this.openGeneration;
    this.noteLoading = true;
    this.loading = true;
    this.error = null;
    this.clearProposal();
    this.closeAttachmentPreview();
    try {
      const content = await readAbsoluteTextFile(trimmed);
      if (openGen !== this.openGeneration) return false;
      const name = fileNameFromAbsolutePath(trimmed);
      const title = name.replace(/\.md$/i, "").replace(/\.markdown$/i, "") || name;
      this.clearLooseFile();
      // Lease transfer before body apply — remounts/saves target this path.
      this.looseFilePath = trimmed;
      this.selectedPath = trimmed;
      this.resetSaveState();
      this.content = content;
      this.baselineContent = content;
      this.contentHash = null;
      this.title = title;
      this.selectedKind = "note";
      this.wikilinksOut = [];
      this.backlinks = [];
      this.noteTags = [];
      this.dirty = false;
      this.editorMode = "edit";
      this.bumpContentSync();
      await this.syncLmeNoteTab(trimmed);
      return true;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return false;
    } finally {
      this.noteLoading = false;
      this.loading = false;
    }
  }

  /** Bind the LME keep-alive host to the focused note (create/activate tab). */
  private async syncLmeNoteTab(path: string) {
    try {
      const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
      lmeWorkspace.ensureAndActivateNoteTab(path);
    } catch {
      // Unit tests / non-shell contexts may not load the LME workspace.
    }
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
            isObsidian: false,
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
    this.clearLooseFile();
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

  /**
   * Flush TipTap/CM drafts + save the leaving note before remount/activate.
   * On failure (conflict/error), stays on the current note — caller must not
   * change activeTabId.
   */
  async flushBeforeLeave(options?: { skipEditorFlush?: boolean }): Promise<boolean> {
    if (!options?.skipEditorFlush) {
      await invokeVaultLeaveFlush();
    }
    if (!this.selectedPath && !this.looseFilePath) return true;
    if (!this.dirty) {
      this.stashSelectedBuffer();
      this.stashFocusedEditorUi();
      return true;
    }
    const ok = await this.save({ source: "autosave" });
    if (ok) {
      this.stashSelectedBuffer();
      this.stashFocusedEditorUi();
    }
    return ok;
  }

  /** Bumped when openNote restores UI — editors apply scrollTop from runtime. */
  editorScrollRestoreEpoch = $state(0);
  editorScrollRestorePath = $state<string | null>(null);
  editorScrollRestoreTop = $state(0);

  /**
   * Folder-tree expand map (session). Key = folder path / space id / name.
   * Survives Workspace rail remounts.
   */
  treeExpandedByKey = $state<Record<string, boolean>>({});

  treeExpandKeyFor(node: {
    path?: string | null;
    spaceId?: string | null;
    dropPrefix?: string | null;
    name: string;
  }): string {
    return (node.path ?? node.spaceId ?? node.dropPrefix ?? node.name).trim();
  }

  isTreeExpanded(key: string): boolean | undefined {
    const normalized = key.trim();
    if (!normalized) return undefined;
    if (Object.prototype.hasOwnProperty.call(this.treeExpandedByKey, normalized)) {
      return this.treeExpandedByKey[normalized];
    }
    return undefined;
  }

  setTreeExpanded(key: string, expanded: boolean) {
    const normalized = key.trim();
    if (!normalized) return;
    this.treeExpandedByKey = {
      ...this.treeExpandedByKey,
      [normalized]: expanded,
    };
  }

  stashEditorScroll(path: string | null | undefined, scrollTop: number) {
    const key = path?.trim();
    if (!key || this.isLooseFile) return;
    noteEditorRuntimes.patchUi(key, {
      scrollTop: Math.max(0, scrollTop),
    });
  }

  private stashFocusedEditorUi(scrollTop?: number) {
    const path = this.selectedPath?.trim();
    if (!path || this.isLooseFile) return;
    noteEditorRuntimes.patchUi(path, {
      plane: this.notePlane,
      editorMode: this.editorMode,
      editorSurface: this.editorSurface,
      ...(typeof scrollTop === "number" ? { scrollTop: Math.max(0, scrollTop) } : {}),
    });
  }

  private restoreEditorUi(path: string) {
    const runtime = noteEditorRuntimes.ensure(path, {
      plane: this.notePlane,
      editorMode: this.editorMode,
      editorSurface: this.editorSurface,
    });
    noteEditorRuntimes.touch(path);
    this.notePlane = runtime.ui.plane;
    this.editorMode = runtime.ui.editorMode;
    this.editorSurface = runtime.ui.editorSurface;
    this.editorScrollRestorePath = path;
    this.editorScrollRestoreTop = runtime.ui.scrollTop ?? 0;
    this.editorScrollRestoreEpoch += 1;
  }

  async openNote(path: string, options?: { skipLeaveFlush?: boolean }) {
    const nextPath = this.normalizeNotePath(path);
    if (!nextPath) return;

    if (this.selectedPath === nextPath && !this.isLooseFile && !this.noteLoading) {
      const hasSession =
        this.contentHash != null ||
        this.dirty ||
        this.noteBuffers.has(nextPath);
      if (hasSession) {
        noteEditorRuntimes.touch(nextPath);
        return;
      }
    }

    const openGen = ++this.openGeneration;

    if (this.selectedPath && this.selectedPath !== nextPath) {
      // activateTab already flushed the mounted editor — do not flush the remounted host.
      const ok = options?.skipLeaveFlush
        ? await this.flushBeforeLeave({ skipEditorFlush: true })
        : await this.flushBeforeLeave();
      if (!ok) return;
      if (openGen !== this.openGeneration) return;
      this.clearProposal();
      this.closeAttachmentPreview();
    }
    if (openGen !== this.openGeneration) return;
    this.clearLooseFile();

    const buffered = this.noteBuffers.get(nextPath);
    // Buffer-first reopen: dirty or recently stashed clean — skip cold refetch.
    if (buffered) {
      this.selectedPath = nextPath;
      this.restoreBufferIntoFocused(buffered);
      this.restoreEditorUi(nextPath);
      localStorage.setItem(LAST_NOTE_KEY, nextPath);
      rememberVaultRecent(nextPath);
      this.recentPaths = loadVaultRecent();
      this.rememberSpaceForPath(nextPath, buffered.title);
      await this.refreshBacklinks(nextPath);
      return;
    }

    // Quiescent handoff: take the write lease on `nextPath` *before* applying
    // body so remounts / destroy flushes / autosave cannot PUT onto the old path.
    this.noteLoading = true;
    this.loading = true;
    this.error = null;
    this.selectedPath = nextPath;
    this.dirty = false;
    this.resetSaveState();
    try {
      const response: VaultNoteContentResponse = await getVaultNote(nextPath);
      if (openGen !== this.openGeneration || this.selectedPath !== nextPath) {
        return;
      }
      this.applyNote(response);
      this.writeBufferFromResponse(nextPath, response);
      this.restoreEditorUi(nextPath);
      localStorage.setItem(LAST_NOTE_KEY, nextPath);
      rememberVaultRecent(nextPath);
      this.recentPaths = loadVaultRecent();
      this.rememberSpaceForPath(nextPath, response.note.title);
      await this.refreshBacklinks(nextPath);
    } catch (err) {
      if (openGen === this.openGeneration && this.selectedPath === nextPath) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (openGen === this.openGeneration) {
        this.noteLoading = false;
        this.loading = false;
        if (this.pendingHeadingScroll) {
          this.headingScrollRequest += 1;
        }
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
    if (noteHasSlidesDeck(response.content) || this.selectedKind === "slides") {
      this.deckEditMode = "deck";
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
    if (this.selectedPath && !this.isLooseFile) {
      noteEditorRuntimes.patchUi(this.selectedPath, { editorMode: mode });
    }
  }

  setEditorSurface(surface: "write" | "source") {
    this.editorSurface = surface;
    saveEditorSurface(surface);
    if (this.selectedPath && !this.isLooseFile) {
      noteEditorRuntimes.patchUi(this.selectedPath, { editorSurface: surface });
    }
  }

  toggleEditorSurface() {
    this.setEditorSurface(this.editorSurface === "write" ? "source" : "write");
  }

  setNotePlane(plane: VaultNotePlane) {
    this.notePlane = plane;
    saveNotePlane(plane);
    if (this.selectedPath && !this.isLooseFile) {
      noteEditorRuntimes.patchUi(this.selectedPath, { plane });
    }
    if (plane === "live") {
      this.setEditorSurface("write");
    }
  }

  /**
   * Sticky popout: force Live + write surface without writing prefs
   * so the main window's plane/surface stay intact.
   */
  applyStickyLivePlane() {
    this.notePlane = "live";
    this.editorSurface = "write";
  }

  toggleNotePlane() {
    this.setNotePlane(this.notePlane === "live" ? "build" : "live");
  }

  enterEditMode() {
    // Prefer split for markdown notes that support preview (layout.vaultSplitEnabled
    // defaults true). Never force split off when returning to edit.
    this.setEditorMode("edit");
  }

  enterPreviewMode() {
    this.setEditorMode("preview");
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
    if (this.isLooseFile) {
      this.backlinks = [];
      return;
    }
    try {
      const response = await getVaultBacklinks(path);
      this.backlinks = response.backlinks;
    } catch {
      // Non-fatal — note metadata may still have backlinks.
    }
  }

  /**
   * Update note markdown (source of truth).
   * Editor keystrokes must NOT bump contentSyncKey — that remounts Live/Build and
   * causes stale TipTap flushes to clobber the open note. Pass `reloadEditors: true`
   * only for out-of-band mutations (attachments, bridges, preview toggles).
   *
   * Pass `path` from the emitting editor so remounts / keep-alive hosts cannot
   * write into a different leased session.
   */
  markDirty(
    nextContent: string,
    options?: { reloadEditors?: boolean; allowEmpty?: boolean; path?: string | null },
  ) {
    if (this.noteLoading) {
      return;
    }
    if (options?.path != null && options.path.trim() !== "") {
      if (!this.isFocusedPath(options.path)) {
        return;
      }
    }
    // Live serialize/organism remounts must not mark dirty on open with no edits.
    if (nextContent === this.content) {
      return;
    }
    if (!options?.allowEmpty && this.shouldRefuseEmptyOverwrite(this.content, nextContent)) {
      return;
    }
    this.content = nextContent;
    this.dirty = true;
    const { frontmatter } = stripFrontmatter(nextContent);
    const fmTitle = parseFrontmatterTitle(frontmatter).trim();
    if (fmTitle) {
      this.title = fmTitle;
    }
    if (options?.reloadEditors) {
      this.bumpContentSync();
    }
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

  /**
   * Persist a non-focused (or focused) path through the per-path save queue
   * with If-Match — never bypass versioning for embed write-through.
   */
  async saveNoteAtPath(
    path: string,
    content: string,
    options?: { force?: boolean },
  ): Promise<boolean> {
    const key = this.normalizeNotePath(path);
    if (!key) return false;
    const hash = this.isFocusedPath(key)
      ? this.contentHash
      : (this.noteBuffers.get(key)?.contentHash ?? null);
    const result = await this.saveQueue.enqueue(key, {
      content,
      contentHash: options?.force ? null : hash,
      force: Boolean(options?.force),
      source: "manual",
    });
    return result.ok;
  }

  /** Refuse empty/near-empty Live serialize over a substantial note body. */
  private shouldRefuseEmptyOverwrite(previous: string, next: string): boolean {
    const prevSig = significantLiveText(previous);
    const nextSig = significantLiveText(next);
    if (prevSig.length <= 20) return false;
    if (nextSig.length === 0) return true;
    if (prevSig.length > 40 && nextSig.length < 3 && nextSig.length < prevSig.length * 0.05) {
      return true;
    }
    return false;
  }

  /** Update note kind in frontmatter and chrome immediately. */
  setNoteKind(kind: VaultNoteKind) {
    if (!this.selectedPath || this.isLooseFile) return;
    const next = normalizeKind(kind);
    this.selectedKind = next;
    this.markDirty(setFrontmatterKind(this.content, next), {
      reloadEditors: true,
    });
  }

  private applySaveResponse(
    response: VaultNoteContentResponse["note"],
    writtenContent?: string | null,
  ) {
    this.contentHash = response.content_hash;
    this.title = response.title;
    this.selectedKind = resolveKind(response.path, response.kind);
    this.wikilinksOut = response.wikilinks_out;
    this.noteTags = response.tags ?? [];
    // Prefer server body echo (semantic tags) so baseline matches disk.
    if (typeof writtenContent === "string") {
      this.content = writtenContent;
      this.baselineContent = writtenContent;
    } else {
      this.baselineContent = this.content;
    }
    this.dirty = false;
  }

  private patchBufferAfterSave(
    path: string,
    writtenContent: string,
    note: VaultNoteContentResponse["note"],
  ) {
    const key = this.normalizeNotePath(path);
    const prior = this.noteBuffers.get(key);
    if (this.isFocusedPath(key)) {
      this.noteBuffers.set(key, {
        path: key,
        content: this.content,
        baselineContent: writtenContent,
        contentHash: note.content_hash,
        title: note.title,
        dirty: this.dirty,
        contentRevision: this.contentRevision,
      });
    } else {
      const content = prior?.content ?? writtenContent;
      this.noteBuffers.set(key, {
        path: key,
        content,
        baselineContent: writtenContent,
        contentHash: note.content_hash,
        title: note.title,
        dirty: content !== writtenContent,
        contentRevision: prior?.contentRevision ?? 0,
      });
    }
    this.bumpNoteBuffers();
  }

  private async runSaveJob(path: string, job: NoteSaveJob): Promise<NoteSaveResult> {
    try {
      const response = await saveVaultNote(path, job.content, {
        contentHash: job.force ? undefined : (job.contentHash ?? undefined),
        sessionId: workshopSessionIdForVaultSave(path),
      });
      const written = response.content ?? job.content;

      if (this.isFocusedPath(path)) {
        this.contentHash = response.note.content_hash;
        this.title = response.note.title;
        this.selectedKind = resolveKind(response.note.path, response.note.kind);
        this.wikilinksOut = response.note.wikilinks_out;
        this.noteTags = response.note.tags ?? [];
        if (this.content === job.content || this.content === written) {
          this.content = written;
          this.baselineContent = written;
          this.dirty = false;
        } else {
          // Typed during in-flight save — keep newer draft dirty vs written baseline.
          this.baselineContent = written;
          this.dirty = true;
        }
      }

      this.patchBufferAfterSave(path, written, response.note);

      invalidateMedousaViewCache(path);
      invalidateTransclusionCache(path);
      this.markSaveEcho(path);
      this.scheduleNotesRefresh();
      if (this.isFocusedPath(path)) {
        void this.refreshBacklinks(path);
      }

      return {
        ok: true,
        contentHash: response.note.content_hash,
        writtenContent: written,
      };
    } catch (err) {
      if (isVaultConflictError(err)) {
        if (this.isFocusedPath(path)) {
          this.saveStatus = "conflict";
          this.conflictMessage =
            "This note changed on disk. Reload the latest version or keep your edits.";
        }
        return {
          ok: false,
          conflict: true,
          error: "conflict",
        };
      }
      const message = err instanceof Error ? err.message : String(err);
      if (this.isFocusedPath(path)) {
        this.error = message;
      }
      return { ok: false, error: message };
    }
  }

  async save(options?: { force?: boolean; source?: "manual" | "autosave" }) {
    if (!this.selectedPath) return false;
    if (this.noteLoading && options?.source === "autosave") return false;
    if (!this.dirty && !options?.force) return true;
    if (this.proposalActive && !options?.force) return false;

    this.clearAutosaveTimer();
    this.saving = true;
    this.saveStatus = "saving";
    this.error = null;

    const pathSnapshot = this.selectedPath;
    const contentSnapshot = this.content;
    const loosePath = this.looseFilePath;

    try {
      if (loosePath) {
        await writeAbsoluteTextFile(loosePath, contentSnapshot);
        if (this.selectedPath !== pathSnapshot || this.looseFilePath !== loosePath) {
          return true;
        }
        if (this.content !== contentSnapshot) {
          this.baselineContent = contentSnapshot;
          this.dirty = true;
          this.saveStatus = "unsaved";
          this.scheduleAutosave();
          return true;
        }
        this.baselineContent = this.content;
        this.dirty = false;
        this.clearProposal();
        this.flashSavedWhisper();
        return true;
      }

      const result = await this.saveQueue.enqueue(pathSnapshot, {
        content: contentSnapshot,
        contentHash: options?.force ? null : this.contentHash,
        force: Boolean(options?.force),
        source: options?.source ?? "manual",
      });

      if (this.selectedPath !== pathSnapshot) return result.ok;

      if (!result.ok) {
        if (!result.conflict) {
          this.saveStatus = "unsaved";
          this.scheduleAutosave();
        }
        return false;
      }

      if (this.dirty) {
        this.saveStatus = "unsaved";
        this.scheduleAutosave();
        return true;
      }

      this.clearProposal();
      this.flashSavedWhisper();
      return true;
    } catch (err) {
      this.saveStatus = "unsaved";
      this.error = err instanceof Error ? err.message : String(err);
      this.scheduleAutosave();
      return false;
    } finally {
      this.saving = this.selectedPath
        ? this.saveQueue.isBusy(this.selectedPath)
        : false;
    }
  }

  async flushSave() {
    await invokeVaultLeaveFlush();
    return this.save({ source: "manual" });
  }

  async reloadFromServer() {
    if (!this.selectedPath) return;
    if (this.looseFilePath) {
      this.noteLoading = true;
      this.error = null;
      try {
        const content = await readAbsoluteTextFile(this.looseFilePath);
        this.content = content;
        this.baselineContent = content;
        this.dirty = false;
        this.resetSaveState();
        this.clearProposal();
        this.bumpContentSync();
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      } finally {
        this.noteLoading = false;
      }
      return;
    }
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
        await this.syncLmeNoteTab(response.note.path);
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
    this.markDirty(insertTextAtSection(this.content, "## Links", link), {
      reloadEditors: true,
    });
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
    if (this.isLooseFile) return;
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
    this.markDirty(nextContent, { reloadEditors: true });
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

  toggleDeckEditMode() {
    this.deckEditMode = this.deckEditMode === "deck" ? "raw" : "deck";
  }

  setDeckEditMode(mode: "deck" | "raw") {
    this.deckEditMode = mode;
  }

  async linkAttachmentFiles() {
    if (!this.selectedPath) return;
    const picked = await pickAttachmentFiles();
    if (picked.length === 0) return;
    this.markDirty(addAttachments(this.content, picked), {
      reloadEditors: true,
    });
  }

  async linkSpreadsheetFiles() {
    if (!this.selectedPath) return;
    const picked = await pickSpreadsheetFiles();
    if (picked.length === 0) return;
    this.markDirty(addAttachments(this.content, picked), {
      reloadEditors: true,
    });
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
      { reloadEditors: true },
    );
    return true;
  }

  removeAttachment(path: string) {
    if (!this.selectedPath) return;
    this.markDirty(dropAttachment(this.content, path), {
      reloadEditors: true,
    });
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

function loadNotePlane(): VaultNotePlane {
  if (typeof localStorage === "undefined") return "live";
  try {
    const raw = localStorage.getItem(NOTE_PLANE_KEY);
    if (raw === "live" || raw === "build") return raw;
  } catch {
    /* ignore */
  }
  return "live";
}

function saveNotePlane(plane: VaultNotePlane) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(NOTE_PLANE_KEY, plane);
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
  // Default Recent — notes first, structure on request (Folders stays one click away).
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
