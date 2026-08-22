import {
  createVaultNote,
  deleteVaultNote,
  getVaultBacklinks,
  getVaultNote,
} from "$lib/daemon";
import {
  countNotesBySpace,
  getSpaceById,
  loadLastSpace,
  loadShowSystemNotes,
  resolveSpaceForPath,
} from "$lib/config/vaultSpaces";
import {
  readVaultBuildAutoSave,
  readVaultBuildLineNumbers,
  readVaultBuildScrollSync,
  readVaultBuildWordWrap,
  readVaultStampCompletionEnabled,
  readVaultHideLiveMarkdownSyntax,
  readVaultPaperWidth,
  readVaultReadingPalette,
  type VaultPaperWidth,
  type VaultReadingPalette,
} from "$lib/config/vaultPreferences";
import type {
  VaultNote,
  VaultNoteContentResponse,
  VaultRootView,
  VaultSearchHit,
  VaultTreeNode,
} from "$lib/types/vault";
import { buildVaultLabelMap } from "$lib/utils/formatVault";
import {
  contentForTemplate,
  dailyNotePath,
  dailyNoteTemplate,
  inboxCapturePath,
  inboxCaptureTemplate,
  folderPrefixFromNotePath,
  joinVaultFolder,
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
  resolveKind,
  setFrontmatterKind,
  type VaultNoteKind,
} from "$lib/utils/vaultFrontmatter";
import { parseWikilinkTarget, resolveWikilinkTarget, suggestPathForWikilinkToken } from "$lib/utils/resolveWikilink";
import {
  buildVaultLookupSnapshot,
  withSelectionAncestors,
  type VaultLookupSnapshot,
} from "$lib/utils/vaultLookup";
import { publishVaultLookupSnapshot } from "$lib/vault/vaultLookupLive";
import { setVaultNoteBufferPort } from "$lib/vault/vaultNoteBufferPort";
import { publishVaultDirty } from "$lib/runtime/vaultDirtySnapshot";
import {
  addAttachments,
  guessMimeFromPath,
  listAttachments,
  removeAttachment as dropAttachment,
  type VaultAttachment,
} from "$lib/utils/vaultAttachments";
import { pickAttachmentFiles, pickSpreadsheetFiles } from "$lib/utils/vaultAttachmentPicker";
import { isWriteFirstKind } from "$lib/utils/vaultAuthoring";
import { type VaultSaveStatus, vaultIfMatchToken } from "$lib/utils/vaultSave";
import {
  isAbsoluteDiskPath,
  normalizeVaultNotePath,
  setNoteTitleInContent,
} from "$lib/utils/vaultNoteTitle";
import {
  ensureDataFirstSurface,
  kindFromNoteContent,
} from "$lib/utils/dataFirstSurface";
import { type NoteBuffer } from "$lib/stores/noteBuffer";
import { noteEditorRuntimes } from "$lib/vault/noteEditorRuntimes.svelte";
import { vaultOverlay } from "$lib/vault/vaultOverlay.svelte";
import {
  VaultBrowseController,
  loadLibraryBrowseMode,
  type LibraryBrowseMode,
  type VaultTagCount,
} from "$lib/vault/vaultBrowseController";
import {
  VaultEditorController,
  type VaultNotePlane,
  type VaultProposalSource,
} from "$lib/vault/vaultEditorController";
import { VaultBridgeController } from "$lib/vault/vaultBridgeController";
import { VaultRootsController } from "$lib/vault/vaultRootsController";
import { VaultRailController } from "$lib/vault/vaultRailController";
import type { MedousaViewQuery } from "$lib/utils/markdownView";
import type { ChartFenceKv } from "$lib/utils/vaultChartFence";
import type { LiquidFenceDraft, LiquidFenceLang } from "$lib/utils/vaultLiquidFence";
import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";
import type { WorkspaceEvent } from "$lib/types/workspace";
import { loadVaultRecent, rememberVaultRecent } from "$lib/utils/vaultRecent";
import {
  formatDiffChip,
  lineDiffStats,
  type LineDiffStats,
} from "$lib/utils/vaultDiff";

const LAST_NOTE_KEY = "medousa-home-last-note";
const EDITOR_SURFACE_KEY = "medousa-home-vault-editor-surface";
const NOTE_PLANE_KEY = "medousa-home-vault-note-plane";

export type { LibraryBrowseMode, VaultTagCount, VaultNotePlane, VaultProposalSource };

export class VaultStore {
  notes = $state<VaultNote[]>([]);
  /** Shared per-generation lookup maps — rebuilt when notes/generation change. */
  lookupSnapshot = $state<VaultLookupSnapshot>(buildVaultLookupSnapshot([], 0));
  vaultGeneration = $state(0);
  listingIncomplete = $state(false);
  tree = $state<VaultTreeNode[]>([]);
  selectedPath = $state<string | null>(loadLastNote());
  /** Multi-select in the vault rail (tree / browse lists). */
  selectedPaths = $state<Set<string>>(new Set());
  /** Anchor for shift-click range selection. */
  selectionAnchorPath = $state<string | null>(null);
  /** Absolute path when editing a single .md outside any vault root. */
  looseFilePath = $state<string | null>(null);
  content = $state("");
  baselineContent = $state("");
  contentHash = $state<string | null>(null);
  noteBufferRevision = $state(0);
  wikilinksOut = $state<string[]>([]);
  backlinks = $state<string[]>([]);
  noteTags = $state<string[]>([]);
  title = $state("");
  selectedKind = $state<VaultNoteKind>("note");
  private dirtyState = $state(false);
  get dirty(): boolean {
    return this.dirtyState;
  }
  set dirty(next: boolean) {
    this.dirtyState = next;
    publishVaultDirty(next);
  }
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
  /** Workbook marker: manifest view vs raw markdown. */
  workbookEditMode = $state<"view" | "raw">("view");
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
  /** Live: hide focused heading `#` widgets. */
  hideLiveMarkdownSyntax = $state(readVaultHideLiveMarkdownSyntax());
  /** Live / Preview paper column width. */
  paperWidth = $state<VaultPaperWidth>(readVaultPaperWidth());
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
  get garageWizardOpen() {
    return vaultOverlay.garageWizardOpen;
  }
  set garageWizardOpen(value: boolean) {
    vaultOverlay.garageWizardOpen = value;
  }
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
  /** Bumped when openNote restores UI — editors apply scrollTop from runtime. */
  editorScrollRestoreEpoch = $state(0);
  editorScrollRestorePath = $state<string | null>(null);
  editorScrollRestoreTop = $state(0);
  /**
   * Folder-tree expand map (session). Key = folder path / space id / name.
   * Survives Workspace rail remounts.
   */
  treeExpandedByKey = $state<Record<string, boolean>>({});
  openGeneration = 0;
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

  #browse: VaultBrowseController;
  #editor: VaultEditorController;
  #bridge: VaultBridgeController;
  #roots: VaultRootsController;
  #rail: VaultRailController;

  constructor() {
    this.#browse = new VaultBrowseController(this);
    this.#editor = new VaultEditorController(this);
    this.#bridge = new VaultBridgeController(this);
    this.#roots = new VaultRootsController(this);
    this.#rail = new VaultRailController(this);
  }

  rebuildTree() { this.#browse.rebuildTree(); }
  setLibraryBrowseMode(mode: LibraryBrowseMode) { this.#browse.setLibraryBrowseMode(mode); }
  scopedLibraryNotes() { return this.#browse.scopedLibraryNotes(); }
  notesForTag(tag: string) { return this.#browse.notesForTag(tag); }
  notesByKind() { return this.#browse.notesByKind(); }
  recentNotesList(limit?: number) { return this.#browse.recentNotesList(limit); }
  refreshVaultTags() { return this.#browse.refreshVaultTags(); }
  setShowAgentReviewFilter(value: boolean) { this.#browse.setShowAgentReviewFilter(value); }
  setShowSystemNotes(value: boolean) { this.#browse.setShowSystemNotes(value); }
  setActiveSpaceFilter(spaceId: string | null) { this.#browse.setActiveSpaceFilter(spaceId); }
  focusSpaceForPath(path: string, title: string) { this.#browse.focusSpaceForPath(path, title); }
  applySpaceFilterAfterMove(path: string, title: string, filterWasAll: boolean) {
    this.#browse.applySpaceFilterAfterMove(path, title, filterWasAll);
  }
  addCustomGroup(label: string) { return this.#browse.addCustomGroup(label); }
  scheduleNotesRefresh() { this.#browse.scheduleNotesRefresh(); }
  refreshNotes() { return this.#browse.refreshNotes(); }
  runSearch(query: string) { return this.#browse.runSearch(query); }

  clearAutosaveTimer() { this.#editor.clearAutosaveTimer(); }
  clearSavedWhisperTimer() { this.#editor.clearSavedWhisperTimer(); }
  scheduleAutosave() { this.#editor.scheduleAutosave(); }
  setCompositionHold(active: boolean) { this.#editor.setCompositionHold(active); }
  flashSavedWhisper() { this.#editor.flashSavedWhisper(); }
  resetSaveState() { this.#editor.resetSaveState(); }
  contentFor(path: string) { return this.#editor.contentFor(path); }
  contentSyncKeyFor(path: string) { return this.#editor.contentSyncKeyFor(path); }
  titleFor(path: string) { return this.#editor.titleFor(path); }
  noteLoadingFor(path: string) { return this.#editor.noteLoadingFor(path); }
  seedBufferForTest(buffer: NoteBuffer) { this.#editor.seedBuffer(buffer); }
  warmBuffer(path: string) { return this.#editor.warmBuffer(path); }
  proposalDiffStats() { return this.#editor.proposalDiffStats(); }
  noteFromFeedEvent(event: WorkspaceEvent) {
    this.#editor.noteFromFeedEvent(event);
  }
  ingestRemoteUpdate(event: WorkspaceEvent) {
    return this.#editor.ingestRemoteUpdate(event);
  }
  acceptProposal() { return this.#editor.acceptProposal(); }
  discardProposal() { return this.#editor.discardProposal(); }
  editProposal() { this.#editor.editProposal(); }
  setStampCompletionInline(value: boolean) { this.#editor.setStampCompletionInline(value); }
  setBuildWordWrap(value: boolean) { this.#editor.setBuildWordWrap(value); }
  setBuildLineNumbers(value: boolean) { this.#editor.setBuildLineNumbers(value); }
  setBuildAutoSave(value: boolean) { this.#editor.setBuildAutoSave(value); }
  setBuildScrollSync(value: boolean) { this.#editor.setBuildScrollSync(value); }
  setReadingPalette(palette: VaultReadingPalette) {
    this.#editor.setReadingPalette(palette);
  }
  cycleReadingPalette() { this.#editor.cycleReadingPalette(); }
  setHideLiveMarkdownSyntax(enabled: boolean) { this.#editor.setHideLiveMarkdownSyntax(enabled); }
  toggleHideLiveMarkdownSyntax() { this.#editor.toggleHideLiveMarkdownSyntax(); }
  setPaperWidth(width: VaultPaperWidth) {
    this.#editor.setPaperWidth(width);
  }
  cyclePaperWidth() { this.#editor.cyclePaperWidth(); }
  togglePreviewTask(taskIndex: number, checked: boolean) {
    this.#editor.togglePreviewTask(taskIndex, checked);
  }
  queueEditorInsert(text: string) { this.#editor.queueEditorInsert(text); }
  takeEditorInsert() { return this.#editor.takeEditorInsert(); }
  flushBeforeLeave(options?: { skipEditorFlush?: boolean }) {
    return this.#editor.flushBeforeLeave(options);
  }
  markDirty(
    nextContent: string,
    options?: { reloadEditors?: boolean; allowEmpty?: boolean; path?: string | null },
  ) {
    this.#editor.markDirty(nextContent, options);
  }
  saveNoteAtPath(path: string, content: string, options?: { force?: boolean }) {
    return this.#editor.saveNoteAtPath(path, content, options);
  }
  save(options?: { force?: boolean; source?: "manual" | "autosave" }) {
    return this.#editor.save(options);
  }
  flushSave() { return this.#editor.flushSave(); }
  reloadFromServer() { return this.#editor.reloadFromServer(); }
  keepMineAndSave() { return this.#editor.keepMineAndSave(); }
  clearProposal() { this.#editor.clearProposal(); }
  getBuffer(path: string) { return this.#editor.getBuffer(path); }
  deleteBuffer(path: string) { this.#editor.deleteBuffer(path); }
  writeAbsoluteBuffer(path: string, content: string, title: string) {
    this.#editor.writeAbsoluteBuffer(path, content, title);
  }
  restoreBufferIntoFocused(buffer: NoteBuffer) {
    this.#editor.restoreBufferIntoFocused(buffer);
  }
  resetEditorBuffers() { this.#editor.resetEditorBuffers(); }

  openViewBridgeInsert(insertAt: number) { this.#bridge.openViewBridgeInsert(insertAt); }
  openViewBridgeEdit(index: number) { this.#bridge.openViewBridgeEdit(index); }
  closeViewBridge() { this.#bridge.closeViewBridge(); }
  commitViewBridge(query: MedousaViewQuery) { this.#bridge.commitViewBridge(query); }
  openChartBridgeEdit(index: number) { this.#bridge.openChartBridgeEdit(index); }
  closeChartBridge() { this.#bridge.closeChartBridge(); }
  commitChartBridge(kv: ChartFenceKv, tableMarkdown?: string) {
    this.#bridge.commitChartBridge(kv, tableMarkdown);
  }
  openLiquidBridgeEdit(lang: LiquidFenceLang, index: number) {
    this.#bridge.openLiquidBridgeEdit(lang, index);
  }
  closeLiquidBridge() { this.#bridge.closeLiquidBridge(); }
  commitLiquidBridge(next: LiquidFenceDraft) { this.#bridge.commitLiquidBridge(next); }
  openCardDetail(detail: CardDetailPayload) { this.#bridge.openCardDetail(detail); }
  closeCardDetail() { this.#bridge.closeCardDetail(); }
  insertImageEmbed(imagePath: string) { return this.#bridge.insertImageEmbed(imagePath); }

  resetForWorkshopSwitch() { this.#roots.resetForWorkshopSwitch(); }
  clearLooseFile() { this.#roots.clearLooseFile(); }
  openLooseMarkdownFile() { return this.#roots.openLooseMarkdownFile(); }
  openLooseFile(absolutePath: string, options?: { skipLeaveFlush?: boolean }) {
    return this.#roots.openLooseFile(absolutePath, options);
  }
  refreshVaultRoots() { return this.#roots.refreshVaultRoots(); }
  switchVaultRoot(rootId: string) { return this.#roots.switchVaultRoot(rootId); }
  registerVaultRoot(label: string, path: string) {
    return this.#roots.registerVaultRoot(label, path);
  }
  openAddVaultRootDialog() { this.#roots.openAddVaultRootDialog(); }
  closeAddVaultRootDialog() { this.#roots.closeAddVaultRootDialog(); }

  selectionAncestorSet() { return this.#rail.selectionAncestorSet(); }
  isSelectionAncestor(pathOrFolder: string | null) {
    return this.#rail.isSelectionAncestor(pathOrFolder);
  }
  treeExpandKeyFor(node: {
    path?: string | null;
    spaceId?: string | null;
    dropPrefix?: string | null;
    name: string;
  }) {
    return this.#rail.treeExpandKeyFor(node);
  }
  isTreeExpanded(key: string) { return this.#rail.isTreeExpanded(key); }
  setTreeExpanded(key: string, expanded: boolean) { this.#rail.setTreeExpanded(key, expanded); }
  isRailPathSelected(path: string) { return this.#rail.isRailPathSelected(path); }
  clearRailSelection() { this.#rail.clearRailSelection(); }
  railNoteOrder() { return this.#rail.railNoteOrder(); }
  applyRailSelection(path: string, event?: MouseEvent | null) {
    return this.#rail.applyRailSelection(path, event);
  }
  prepareRailContextMenu(path: string) { return this.#rail.prepareRailContextMenu(path); }
  openGarageWizard() { this.#rail.openGarageWizard(); }
  closeGarageWizard() { this.#rail.closeGarageWizard(); }
  finishGarageOnboarding() { this.#rail.finishGarageOnboarding(); }
  shouldPromptGarageWizard() { return this.#rail.shouldPromptGarageWizard(); }

  noteBufferFor(path: string): NoteBuffer | undefined {
    return this.#editor.getBuffer(path);
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

  get currentCreateFolderPrefix(): string | null {
    if (this.isLooseFile) return null;
    return folderPrefixFromNotePath(this.selectedPath);
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

  bumpContentSync() {
    this.contentRevision += 1;
  }

  bumpNoteBuffers() {
    this.noteBufferRevision += 1;
  }

  normalizeNotePath(path: string): string {
    return normalizeVaultNotePath(path.trim()) || path.trim();
  }

  isFocusedPath(path: string | null | undefined): boolean {
    const trimmed = path?.trim();
    if (!trimmed) return false;
    // Loose markdown uses an absolute OS path — do not vault-normalize (strips drive/root).
    if (this.looseFilePath) {
      return trimmed === this.looseFilePath || trimmed === (this.selectedPath?.trim() ?? "");
    }
    if (isAbsoluteDiskPath(trimmed)) {
      return trimmed === (this.selectedPath?.trim() ?? "");
    }
    return this.normalizeNotePath(trimmed) === (this.selectedPath?.trim() ?? "");
  }

  rebuildLookupSnapshot() {
    this.lookupSnapshot = buildVaultLookupSnapshot(
      this.notes,
      this.vaultGeneration,
      this.selectedPath,
    );
    publishVaultLookupSnapshot(this.lookupSnapshot);
  }

  stashEditorScroll(path: string | null | undefined, scrollTop: number) {
    const key = path?.trim();
    if (!key) return;
    noteEditorRuntimes.patchUi(key, {
      scrollTop: Math.max(0, scrollTop),
    });
  }

  stashFocusedEditorUi(scrollTop?: number) {
    const path = this.selectedPath?.trim();
    if (!path) return;
    noteEditorRuntimes.patchUi(path, {
      plane: this.notePlane,
      editorMode: this.editorMode,
      editorSurface: this.editorSurface,
      ...(typeof scrollTop === "number" ? { scrollTop: Math.max(0, scrollTop) } : {}),
    });
  }

  restoreEditorUi(path: string) {
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
    const raw = path.trim();
    if (!raw) return;
    // Absolute OS paths are loose markdown — never vault-normalize / getVaultNote.
    if (isAbsoluteDiskPath(raw)) {
      await this.openLooseFile(raw, { skipLeaveFlush: options?.skipLeaveFlush });
      return;
    }

    const nextPath = this.normalizeNotePath(raw);
    if (!nextPath) return;

    if (this.selectedPath === nextPath && !this.isLooseFile && !this.noteLoading) {
      const hasSession =
        this.contentHash != null ||
        this.dirty ||
        Boolean(this.#editor.getBuffer(nextPath));
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

    const buffered = this.#editor.getBuffer(nextPath);
    // Buffer-first reopen: dirty or recently stashed clean — skip cold refetch.
    if (buffered) {
      this.selectedPath = nextPath;
      this.lookupSnapshot = withSelectionAncestors(this.lookupSnapshot, nextPath);
      publishVaultLookupSnapshot(this.lookupSnapshot);
      this.#editor.restoreBufferIntoFocused(buffered);
      this.restoreEditorUi(nextPath);
      localStorage.setItem(LAST_NOTE_KEY, nextPath);
      rememberVaultRecent(nextPath);
      this.recentPaths = loadVaultRecent();
      await this.refreshBacklinks(nextPath);
      return;
    }

    // Quiescent handoff: take the write lease on `nextPath` *before* applying
    // body so remounts / destroy flushes / autosave cannot PUT onto the old path.
    // Clear body identity so a failed fetch cannot leave the previous note's
    // content under the new path (and so retry is not skipped via contentHash).
    this.noteLoading = true;
    this.loading = true;
    this.error = null;
    this.selectedPath = nextPath;
    this.lookupSnapshot = withSelectionAncestors(this.lookupSnapshot, nextPath);
    publishVaultLookupSnapshot(this.lookupSnapshot);
    this.content = "";
    this.baselineContent = "";
    this.contentHash = null;
    this.title = "";
    this.dirty = false;
    this.resetSaveState();
    this.bumpContentSync();
    try {
      const response: VaultNoteContentResponse = await getVaultNote(nextPath);
      if (openGen !== this.openGeneration || this.selectedPath !== nextPath) {
        return;
      }
      this.applyNote(response);
      this.#editor.writeBufferFromResponse(nextPath, response);
      this.restoreEditorUi(nextPath);
      localStorage.setItem(LAST_NOTE_KEY, nextPath);
      rememberVaultRecent(nextPath);
      this.recentPaths = loadVaultRecent();
      await this.refreshBacklinks(nextPath);
    } catch (err) {
      if (openGen === this.openGeneration && this.selectedPath === nextPath) {
        this.error = err instanceof Error ? err.message : String(err);
        // Drop any placeholder buffer so the next openNote refetches.
        this.#editor.deleteBuffer(nextPath);
        this.bumpNoteBuffers();
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

  syncNoteMetadata(response: VaultNoteContentResponse) {
    this.contentHash = vaultIfMatchToken(response);
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
    this.contentHash = vaultIfMatchToken(response);
    this.title = response.note.title;
    this.selectedKind = kindFromNoteContent(
      response.note.path,
      response.content,
    );
    // Prefer server kind when content has no frontmatter kind yet.
    if (
      this.selectedKind === "note" &&
      response.note.kind &&
      normalizeKind(response.note.kind) !== "note"
    ) {
      this.selectedKind = normalizeKind(response.note.kind);
    }
    this.wikilinksOut = response.note.wikilinks_out;
    this.backlinks = response.note.backlinks;
    this.noteTags = response.note.tags ?? [];
    this.dirty = false;
    this.editorMode = "edit";
    this.#editor.ensureFocusedDataFirstBody();
    this.#editor.applyObjectEditModesForKind(this.selectedKind, this.content);
    this.bumpContentSync();
  }

  defaultEditorMode(path: string, kind?: string): "edit" | "preview" {
    const resolved = resolveKind(path, kind);
    if (isWriteFirstKind(resolved)) return "edit";
    return "edit";
  }

  setEditorMode(mode: "edit" | "preview") {
    this.editorMode = mode;
    if (this.selectedPath) {
      noteEditorRuntimes.patchUi(this.selectedPath, { editorMode: mode });
    }
  }

  setEditorSurface(surface: "write" | "source") {
    this.editorSurface = surface;
    saveEditorSurface(surface);
    if (this.selectedPath) {
      noteEditorRuntimes.patchUi(this.selectedPath, { editorSurface: surface });
    }
  }

  toggleEditorSurface() {
    this.setEditorSurface(this.editorSurface === "write" ? "source" : "write");
  }

  setNotePlane(plane: VaultNotePlane) {
    this.notePlane = plane;
    saveNotePlane(plane);
    if (this.selectedPath) {
      noteEditorRuntimes.patchUi(this.selectedPath, { plane });
    }
    if (plane === "live") {
      this.setEditorSurface("write");
    }
  }

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
    return resolveWikilinkTarget(rawTarget, this.selectedPath, this.lookupSnapshot);
  }

  openWikilink(rawTarget: string) {
    const decoded = decodeURIComponent(rawTarget.trim());
    const { pathToken, heading } = parseWikilinkTarget(decoded);
    // Same-note fragment: `[[#Heading]]` / `[[#^id]]`
    const path =
      !pathToken.trim() && heading
        ? this.selectedPath
        : resolveWikilinkTarget(
            pathToken || decoded,
            this.selectedPath,
            this.lookupSnapshot,
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

  setNoteKind(kind: VaultNoteKind) {
    if (!this.selectedPath || this.isLooseFile) return;
    const next = normalizeKind(kind);
    this.selectedKind = next;
    const ensured = ensureDataFirstSurface(next, this.content, this.title);
    this.#editor.applyObjectEditModesForKind(next, ensured);
    this.markDirty(ensured, {
      reloadEditors: true,
      allowEmpty: next === "workbook",
    });
  }

  async createNote(options: {
    spaceId: string;
    title: string;
    content?: string;
    path?: string;
    templateId?: VaultTemplateId;
    /** Explicit folder (e.g. current working folder). Empty string = vault root. */
    folderPrefix?: string | null;
    /** Optional new subfolder under folderPrefix or the space root. */
    subfolder?: string | null;
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
      let folderForPath: string | undefined;
      if (options.folderPrefix !== undefined && options.folderPrefix !== null) {
        folderForPath = joinVaultFolder(options.folderPrefix, options.subfolder);
      } else if (options.subfolder?.trim()) {
        folderForPath = joinVaultFolder(prefix, options.subfolder);
      }
      const path =
        options.path ??
        pathForTemplate(
          templateId,
          options.spaceId,
          options.title.trim() || slug,
          new Date(),
          folderForPath,
        ) ??
        `${(folderForPath ?? prefix)}${slug}.md`
          .replace(/\/+/g, "/")
          .replace(/^\//, "");
      // Never overwrite an existing note via "create" — same path/title would wipe disk + buffer.
      if (this.notes.some((note) => note.path === path)) {
        this.error = "A note already exists at that path.";
        return null;
      }
      const content =
        options.content ??
        contentForTemplate(
          templateId,
          options.title.trim() || slug,
          new Date(),
          options.spaceId,
        );
      const response = await createVaultNote(path, content);
      if (response.created === false) {
        this.error = "A note already exists at that path.";
        return null;
      }
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
      if (this.notes.some((note) => note.path === newPath)) {
        this.error = "A note already exists at that path.";
        return null;
      }
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
    await this.archiveNotes([path]);
  }

  async archiveNotes(paths: string[]) {
    const unique = [...new Set(paths.map((path) => path.trim()).filter(Boolean))];
    if (unique.length === 0) return;
    this.error = null;
    try {
      for (const path of unique) {
        await deleteVaultNote(path);
      }
      if (this.selectedPath && unique.includes(this.selectedPath)) {
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
      this.clearRailSelection();
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

  toggleWorkbookEditMode() {
    this.workbookEditMode = this.workbookEditMode === "view" ? "raw" : "view";
  }

  setWorkbookEditMode(mode: "view" | "raw") {
    this.workbookEditMode = mode;
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
    this.#rail.syncAttachmentPanelOverlay();
  }

  closeAttachmentPreview() {
    this.previewingAttachmentPath = null;
    this.previewPresentation = "pane";
    this.#rail.syncAttachmentPanelOverlay();
  }

  openNewNoteDialog() {
    this.newNotePrefillTitle = "";
    this.newNotePrefillPath = null;
    this.error = null;
    this.newNoteDialogOpen = true;
  }

  openNewNoteDialogForWikilink(rawTarget: string) {
    const { pathToken } = parseWikilinkTarget(rawTarget);
    const token = pathToken || rawTarget.trim();
    const stem = token.split("/").pop()?.replace(/\.md$/i, "") ?? token;
    this.newNotePrefillTitle = stem.replace(/[-_]+/g, " ");
    this.newNotePrefillPath = suggestPathForWikilinkToken(rawTarget, this.selectedPath);
    this.error = null;
    this.newNoteDialogOpen = true;
  }

  closeNewNoteDialog() {
    this.newNoteDialogOpen = false;
    this.newNotePrefillTitle = "";
    this.newNotePrefillPath = null;
    this.error = null;
  }

  async syncLmeNoteTab(path: string) {
    // Store instances used for isolated previews/tests must not drive the app's
    // singleton workspace (which would perform a second open through `vault`).
    if (this !== vault) return;
    try {
      const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
      lmeWorkspace.ensureAndActivateNoteTab(path);
    } catch {
      // Unit tests / non-shell contexts may not load the LME workspace.
    }
  }

  get activeVaultRoot(): VaultRootView | null {
    return this.activeVaultRootView;
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

export const vault = new VaultStore();
publishVaultLookupSnapshot(vault.lookupSnapshot);
setVaultNoteBufferPort((path) => vault.noteBufferFor(path));
