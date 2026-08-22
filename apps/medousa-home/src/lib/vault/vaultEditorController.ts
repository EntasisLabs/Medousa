import { getVaultNote, saveVaultNote } from "$lib/daemon";
import {
  cycleVaultReadingPalette,
  cycleVaultPaperWidth,
  writeVaultBuildAutoSave,
  writeVaultBuildLineNumbers,
  writeVaultBuildScrollSync,
  writeVaultBuildWordWrap,
  writeVaultHideLiveMarkdownSyntax,
  writeVaultPaperWidth,
  writeVaultReadingPalette,
  writeVaultStampCompletionEnabled,
  type VaultPaperWidth,
  type VaultReadingPalette,
} from "$lib/config/vaultPreferences";
import type { WorkspaceEvent } from "$lib/types/workspace";
import { vaultRefPath } from "$lib/utils/activityEnrichment";
import type { VaultNoteContentResponse } from "$lib/types/vault";
import { listAttachments } from "$lib/utils/vaultAttachments";
import {
  normalizeKind,
  parseFrontmatterKindValue,
  parseFrontmatterTitle,
  resolveKind,
  stripFrontmatter,
  type VaultNoteKind,
} from "$lib/utils/vaultFrontmatter";
import { workshopSessionIdForVaultSave } from "$lib/utils/vaultSaveSession";
import {
  isAbsoluteDiskPath,
  normalizeVaultNotePath,
} from "$lib/utils/vaultNoteTitle";
import { noteHasKanbanBoard } from "$lib/utils/markdownKanban";
import { noteHasSlidesDeck } from "$lib/utils/markdownSlides";
import {
  dataFirstSurfaceReady,
  ensureDataFirstSurface,
  kindFromNoteContent,
} from "$lib/utils/dataFirstSurface";
import { isDataFirstKind } from "$lib/utils/vaultNoteKind";
import { togglePreviewTaskInContent } from "$lib/utils/vaultPreviewTasks";
import { invalidateMedousaViewCache } from "$lib/utils/resolveMedousaViews";
import { type NoteBuffer } from "$lib/stores/noteBuffer";
import {
  NoteSaveQueue,
  type NoteSaveJob,
  type NoteSaveResult,
} from "$lib/vault/noteSaveQueue";
import { invokeVaultLeaveFlush } from "$lib/vault/vaultLeaveFlush";
import { significantLiveText } from "$lib/vault/live/liveSignificantText";
import { invalidateTransclusionCache } from "$lib/utils/resolveTransclusion";
import {
  fileNameFromAbsolutePath,
  readAbsoluteTextFile,
  writeAbsoluteTextFile,
} from "$lib/utils/vaultFilesystem";
import { lineDiffStats, type LineDiffStats } from "$lib/utils/vaultDiff";
import {
  isVaultConflictError,
  vaultIfMatchToken,
  VAULT_AUTOSAVE_MS,
  VAULT_SAVE_ECHO_MS,
  VAULT_SAVED_WHISPER_MS,
  type VaultSaveStatus,
} from "$lib/utils/vaultSave";

export type VaultNotePlane = "live" | "build";
export type VaultProposalSource = "agent" | "operator";

export type VaultEditorHost = {
  selectedPath: string | null;
  looseFilePath: string | null;
  content: string;
  baselineContent: string;
  contentHash: string | null;
  title: string;
  dirty: boolean;
  noteLoading: boolean;
  saving: boolean;
  saveStatus: VaultSaveStatus;
  conflictMessage: string | null;
  error: string | null;
  contentRevision: number;
  noteBufferRevision: number;
  contentSyncKey: string;
  buildAutoSave: boolean;
  proposalActive: boolean;
  proposalContent: string | null;
  proposalSource: VaultProposalSource;
  stampCompletionInline: boolean;
  buildWordWrap: boolean;
  buildLineNumbers: boolean;
  buildScrollSync: boolean;
  readingPalette: VaultReadingPalette;
  hideLiveMarkdownSyntax: boolean;
  paperWidth: VaultPaperWidth;
  pendingEditorInsert: string | null;
  editorInsertRequest: number;
  selectedKind: VaultNoteKind;
  noteTags: string[];
  wikilinksOut: string[];
  backlinks: string[];
  previewingAttachmentPath: string | null;
  previewPresentation: "pane" | "panel";
  agentWrittenAt: Record<string, string>;
  ledgerEditMode: "table" | "raw";
  workbookEditMode: "view" | "raw";
  boardEditMode: "board" | "raw";
  deckEditMode: "deck" | "raw";
  isFocusedPath(path: string | null | undefined): boolean;
  bumpContentSync(): void;
  bumpNoteBuffers(): void;
  applyNote(
    response: VaultNoteContentResponse,
    options?: { preserveProposal?: boolean },
  ): void;
  syncNoteMetadata(response: VaultNoteContentResponse): void;
  scheduleNotesRefresh(): void;
  stashFocusedEditorUi(scrollTop?: number): void;
  closeAttachmentPreview(): void;
  refreshBacklinks(path: string): Promise<void>;
};

export class VaultEditorController {
  #host: VaultEditorHost;
  readonly buffers = new Map<string, NoteBuffer>();
  readonly bufferWarmInFlight = new Set<string>();
  #saveQueue: NoteSaveQueue;
  #autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  #savedWhisperTimer: ReturnType<typeof setTimeout> | null = null;
  #compositionHold = false;
  #saveEchoPath: string | null = null;
  #saveEchoUntil = 0;

  constructor(host: VaultEditorHost) {
    this.#host = host;
    this.#saveQueue = new NoteSaveQueue((path, job) => this.runSaveJob(path, job));
  }

  resetEditorBuffers() {
    this.buffers.clear();
    this.bufferWarmInFlight.clear();
  }

  bufferKey(path: string): string {
    const raw = path.trim();
    if (!raw) return "";
    if (isAbsoluteDiskPath(raw)) return raw;
    return normalizeVaultNotePath(raw) || raw;
  }

  getBuffer(path: string): NoteBuffer | undefined {
    const key = this.bufferKey(path);
    if (!key) return undefined;
    return this.buffers.get(key);
  }

  deleteBuffer(path: string) {
    const key = this.bufferKey(path);
    if (key) this.buffers.delete(key);
  }

  seedBuffer(buffer: NoteBuffer) {
    const key = this.bufferKey(buffer.path);
    this.buffers.set(key, { ...buffer, path: key });
    this.#host.bumpNoteBuffers();
  }

  contentFor(path: string): string {
    void this.#host.noteBufferRevision;
    void this.#host.content;
    const raw = path.trim();
    if (!raw) return "";
    if (this.#host.isFocusedPath(raw)) return this.#host.content;
    const key = this.bufferKey(raw);
    if (!key) return "";
    return this.buffers.get(key)?.content ?? "";
  }

  contentSyncKeyFor(path: string): string {
    void this.#host.noteBufferRevision;
    void this.#host.contentSyncKey;
    const raw = path.trim();
    if (!raw) return "";
    if (this.#host.isFocusedPath(raw)) return this.#host.contentSyncKey;
    const key = this.bufferKey(raw);
    if (!key) return "";
    const buffer = this.buffers.get(key);
    return `${key}:${buffer?.contentRevision ?? 0}`;
  }

  titleFor(path: string): string {
    void this.#host.noteBufferRevision;
    void this.#host.title;
    const raw = path.trim();
    if (!raw) return "";
    if (this.#host.isFocusedPath(raw)) return this.#host.title;
    const key = this.bufferKey(raw);
    if (!key) return "";
    return this.buffers.get(key)?.title ?? "";
  }

  noteLoadingFor(path: string): boolean {
    void this.#host.noteBufferRevision;
    void this.#host.noteLoading;
    const raw = path.trim();
    if (!raw) return false;
    if (this.#host.isFocusedPath(raw)) return this.#host.noteLoading;
    const key = this.bufferKey(raw);
    if (!key) return false;
    const buffer = this.buffers.get(key);
    return !buffer && this.bufferWarmInFlight.has(key);
  }

  stashSelectedBuffer() {
    const host = this.#host;
    const path = host.selectedPath?.trim();
    if (!path) return;
    if (!host.dirty && host.contentHash == null && !host.content.trim()) return;
    const key = this.bufferKey(path);
    if (!key) return;
    this.buffers.set(key, {
      path: key,
      content: host.content,
      baselineContent: host.baselineContent,
      contentHash: host.contentHash,
      title: host.title,
      dirty: host.dirty,
      contentRevision: host.contentRevision,
    });
    host.bumpNoteBuffers();
  }

  writeAbsoluteBuffer(path: string, content: string, title: string) {
    const key = this.bufferKey(path);
    if (!key) return;
    this.buffers.set(key, {
      path: key,
      content,
      baselineContent: content,
      contentHash: null,
      title,
      dirty: false,
      contentRevision: (this.buffers.get(key)?.contentRevision ?? 0) + 1,
    });
    this.#host.bumpNoteBuffers();
  }

  writeBufferFromResponse(path: string, response: VaultNoteContentResponse) {
    const key = normalizeVaultNotePath(path.trim()) || path.trim();
    this.buffers.set(key, {
      path: key,
      content: response.content,
      baselineContent: response.content,
      contentHash: vaultIfMatchToken(response),
      title: response.note.title,
      dirty: false,
      contentRevision: (this.buffers.get(key)?.contentRevision ?? 0) + 1,
    });
    this.#host.bumpNoteBuffers();
  }

  restoreBufferIntoFocused(buffer: NoteBuffer) {
    const host = this.#host;
    host.content = buffer.content;
    host.baselineContent = buffer.baselineContent;
    host.contentHash = buffer.contentHash;
    host.title = buffer.title;
    host.dirty = buffer.dirty;
    host.contentRevision = buffer.contentRevision;
    host.selectedKind = kindFromNoteContent(buffer.path, buffer.content);
    this.ensureFocusedDataFirstBody();
    this.applyObjectEditModesForKind(host.selectedKind, host.content);
  }

  applyObjectEditModesForKind(kind: VaultNoteKind, content: string) {
    const host = this.#host;
    if (kind === "ledger" || kind === "sheet") {
      host.ledgerEditMode = "table";
    }
    if (kind === "workbook") {
      host.workbookEditMode = "view";
    }
    if (noteHasKanbanBoard(content) || kind === "board") {
      host.boardEditMode = "board";
    }
    if (noteHasSlidesDeck(content) || kind === "slides") {
      host.deckEditMode = "deck";
    }
  }

  ensureFocusedDataFirstBody() {
    const host = this.#host;
    const kind = host.selectedKind;
    if (!isDataFirstKind(kind)) return;
    if (dataFirstSurfaceReady(kind, host.content)) return;
    const ensured = ensureDataFirstSurface(kind, host.content, host.title);
    if (ensured === host.content) return;
    host.content = ensured;
    host.dirty = true;
  }

  syncSelectedKindFromContent() {
    const host = this.#host;
    const { frontmatter } = stripFrontmatter(host.content);
    const fmKind = parseFrontmatterKindValue(frontmatter).trim();
    if (!fmKind) return;
    const next = normalizeKind(fmKind);
    if (next !== host.selectedKind) {
      host.selectedKind = next;
    }
  }

  async warmBuffer(path: string) {
    const host = this.#host;
    const raw = path.trim();
    if (!raw) return;
    if (isAbsoluteDiskPath(raw)) {
      if (host.isFocusedPath(raw)) return;
      if (this.buffers.has(raw) || this.bufferWarmInFlight.has(raw)) return;
      this.bufferWarmInFlight.add(raw);
      host.bumpNoteBuffers();
      try {
        const content = await readAbsoluteTextFile(raw);
        if (host.isFocusedPath(raw)) return;
        const name = fileNameFromAbsolutePath(raw);
        const title =
          name.replace(/\.md$/i, "").replace(/\.markdown$/i, "") || name;
        this.writeAbsoluteBuffer(raw, content, title);
      } catch {
        // Leave pane empty; focused openLooseFile will surface errors.
      } finally {
        this.bufferWarmInFlight.delete(raw);
        host.bumpNoteBuffers();
      }
      return;
    }
    const trimmed = normalizeVaultNotePath(raw) || raw;
    if (!trimmed || host.isFocusedPath(trimmed)) return;
    if (this.buffers.has(trimmed) || this.bufferWarmInFlight.has(trimmed)) {
      return;
    }
    this.bufferWarmInFlight.add(trimmed);
    host.bumpNoteBuffers();
    try {
      const response = await getVaultNote(trimmed);
      if (host.isFocusedPath(trimmed)) return;
      this.writeBufferFromResponse(trimmed, response);
    } catch {
      // Leave pane empty; focused openNote will surface errors.
    } finally {
      this.bufferWarmInFlight.delete(trimmed);
      host.bumpNoteBuffers();
    }
  }

  clearAutosaveTimer() {
    if (this.#autosaveTimer) {
      clearTimeout(this.#autosaveTimer);
      this.#autosaveTimer = null;
    }
  }

  clearSavedWhisperTimer() {
    if (this.#savedWhisperTimer) {
      clearTimeout(this.#savedWhisperTimer);
      this.#savedWhisperTimer = null;
    }
  }

  scheduleAutosave() {
    const host = this.#host;
    this.clearAutosaveTimer();
    if (
      !host.buildAutoSave ||
      !host.selectedPath ||
      !host.dirty ||
      host.noteLoading ||
      host.saveStatus === "conflict" ||
      host.proposalActive ||
      this.#compositionHold
    ) {
      return;
    }
    this.#autosaveTimer = setTimeout(() => {
      void this.save({ source: "autosave" });
    }, VAULT_AUTOSAVE_MS);
  }

  setCompositionHold(active: boolean) {
    if (this.#compositionHold === active) return;
    this.#compositionHold = active;
    if (active) {
      this.clearAutosaveTimer();
      return;
    }
    if (this.#host.dirty) {
      this.scheduleAutosave();
    }
  }

  markSaveEcho(path: string) {
    this.#saveEchoPath = path;
    this.#saveEchoUntil = Date.now() + VAULT_SAVE_ECHO_MS;
  }

  shouldIgnoreSaveEcho(event: WorkspaceEvent, path: string): boolean {
    return (
      event.actor === "operator" &&
      path === this.#saveEchoPath &&
      Date.now() < this.#saveEchoUntil &&
      path === this.#host.selectedPath
    );
  }

  flashSavedWhisper() {
    this.#host.saveStatus = "saved";
    this.clearSavedWhisperTimer();
    this.#savedWhisperTimer = setTimeout(() => {
      if (!this.#host.dirty) this.#host.saveStatus = "idle";
    }, VAULT_SAVED_WHISPER_MS);
  }

  resetSaveState() {
    this.clearAutosaveTimer();
    this.clearSavedWhisperTimer();
    this.#host.saveStatus = "idle";
    this.#host.conflictMessage = null;
  }

  clearProposal() {
    this.#host.proposalActive = false;
    this.#host.proposalContent = null;
  }

  proposalDiffStats(): LineDiffStats | null {
    if (!this.#host.proposalContent) return null;
    return lineDiffStats(this.#host.content, this.#host.proposalContent);
  }

  recordAgentWrite(path: string, timestampUtc?: string) {
    this.#host.agentWrittenAt = {
      ...this.#host.agentWrittenAt,
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
      this.#host.scheduleNotesRefresh();
      return;
    }
    void this.ingestRemoteUpdate(event);
    this.#host.scheduleNotesRefresh();
  }

  async ingestRemoteUpdate(event: WorkspaceEvent) {
    const host = this.#host;
    const path = vaultRefPath(event);
    if (!path || path !== host.selectedPath) return;
    if (host.noteLoading) return;

    this.clearAutosaveTimer();
    try {
      const response = await getVaultNote(path);
      const serverContent = response.content;
      const isAgent = event.actor === "agent";

      if (serverContent === host.content) {
        host.syncNoteMetadata(response);
        return;
      }

      if (isAgent || host.dirty || host.proposalActive) {
        host.proposalActive = true;
        host.proposalContent = serverContent;
        host.proposalSource = isAgent ? "agent" : "operator";
        host.contentHash = vaultIfMatchToken(response);
        host.title = response.note.title;
        host.wikilinksOut = response.note.wikilinks_out;
        host.backlinks = response.note.backlinks;
        if (host.dirty && host.saveStatus !== "conflict") {
          host.saveStatus = "unsaved";
        }
        return;
      }

      host.applyNote(response);
    } catch (err) {
      host.error = err instanceof Error ? err.message : String(err);
    }
  }

  async acceptProposal() {
    if (!this.#host.selectedPath || !this.#host.proposalActive) return false;
    this.clearProposal();
    return this.save({ force: true, source: "manual" });
  }

  async discardProposal() {
    const host = this.#host;
    if (!host.selectedPath || !host.proposalContent) return;
    this.clearAutosaveTimer();
    host.content = host.proposalContent;
    host.baselineContent = host.proposalContent;
    host.dirty = false;
    this.resetSaveState();
    this.clearProposal();
    await this.reloadFromServer();
  }

  editProposal() {
    this.#host.proposalActive = false;
  }

  setStampCompletionInline(value: boolean) {
    this.#host.stampCompletionInline = value;
    writeVaultStampCompletionEnabled(value);
  }

  setBuildWordWrap(value: boolean) {
    this.#host.buildWordWrap = value;
    writeVaultBuildWordWrap(value);
  }

  setBuildLineNumbers(value: boolean) {
    this.#host.buildLineNumbers = value;
    writeVaultBuildLineNumbers(value);
  }

  setBuildAutoSave(value: boolean) {
    this.#host.buildAutoSave = value;
    writeVaultBuildAutoSave(value);
    if (value) {
      if (this.#host.dirty) this.scheduleAutosave();
    } else {
      this.clearAutosaveTimer();
    }
  }

  setBuildScrollSync(value: boolean) {
    this.#host.buildScrollSync = value;
    writeVaultBuildScrollSync(value);
  }

  setReadingPalette(palette: VaultReadingPalette) {
    this.#host.readingPalette = palette;
    writeVaultReadingPalette(palette);
  }

  cycleReadingPalette() {
    this.setReadingPalette(cycleVaultReadingPalette(this.#host.readingPalette));
  }

  setHideLiveMarkdownSyntax(enabled: boolean) {
    this.#host.hideLiveMarkdownSyntax = enabled;
    writeVaultHideLiveMarkdownSyntax(enabled);
  }

  toggleHideLiveMarkdownSyntax() {
    this.setHideLiveMarkdownSyntax(!this.#host.hideLiveMarkdownSyntax);
  }

  setPaperWidth(width: VaultPaperWidth) {
    this.#host.paperWidth = width;
    writeVaultPaperWidth(width);
  }

  cyclePaperWidth() {
    this.setPaperWidth(cycleVaultPaperWidth(this.#host.paperWidth));
  }

  togglePreviewTask(taskIndex: number, checked: boolean) {
    const host = this.#host;
    if (!host.selectedPath || host.proposalActive) return;
    const next = togglePreviewTaskInContent(
      host.content,
      taskIndex,
      checked,
      host.stampCompletionInline,
    );
    if (!next || next === host.content) return;
    this.markDirty(next, { reloadEditors: true });
  }

  queueEditorInsert(text: string) {
    this.#host.pendingEditorInsert = text;
    this.#host.editorInsertRequest += 1;
  }

  takeEditorInsert(): string | null {
    const text = this.#host.pendingEditorInsert;
    this.#host.pendingEditorInsert = null;
    return text;
  }

  async flushBeforeLeave(options?: { skipEditorFlush?: boolean }): Promise<boolean> {
    if (!options?.skipEditorFlush) {
      await invokeVaultLeaveFlush();
    }
    const host = this.#host;
    if (!host.selectedPath && !host.looseFilePath) return true;
    if (!host.dirty) {
      this.stashSelectedBuffer();
      host.stashFocusedEditorUi();
      return true;
    }
    const ok = await this.save({ source: "autosave" });
    if (ok) {
      this.stashSelectedBuffer();
      host.stashFocusedEditorUi();
    }
    return ok;
  }

  markDirty(
    nextContent: string,
    options?: { reloadEditors?: boolean; allowEmpty?: boolean; path?: string | null },
  ) {
    const host = this.#host;
    if (host.noteLoading) {
      return;
    }
    if (options?.path != null && options.path.trim() !== "") {
      if (!host.isFocusedPath(options.path)) {
        return;
      }
    }
    if (nextContent === host.content) {
      return;
    }
    if (!options?.allowEmpty && this.shouldRefuseEmptyOverwrite(host.content, nextContent)) {
      return;
    }
    host.content = nextContent;
    host.dirty = true;
    const { frontmatter } = stripFrontmatter(nextContent);
    const fmTitle = parseFrontmatterTitle(frontmatter).trim();
    if (fmTitle) {
      host.title = fmTitle;
    }
    this.syncSelectedKindFromContent();
    if (options?.reloadEditors) {
      host.bumpContentSync();
    }
    if (
      host.previewingAttachmentPath &&
      !listAttachments(nextContent).some(
        (row) => row.path === host.previewingAttachmentPath,
      )
    ) {
      if (host.previewPresentation === "panel") {
        host.closeAttachmentPreview();
      }
    }
    if (host.saveStatus === "conflict") {
      return;
    }
    if (host.saveStatus !== "saving") {
      host.saveStatus = "unsaved";
    }
    this.scheduleAutosave();
  }

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

  async saveNoteAtPath(
    path: string,
    content: string,
    options?: { force?: boolean },
  ): Promise<boolean> {
    const key = normalizeVaultNotePath(path.trim()) || path.trim();
    if (!key) return false;
    const hash = this.#host.isFocusedPath(key)
      ? this.#host.contentHash
      : (this.buffers.get(key)?.contentHash ?? null);
    const result = await this.#saveQueue.enqueue(key, {
      content,
      contentHash: options?.force ? null : hash,
      force: Boolean(options?.force),
      source: "manual",
    });
    return result.ok;
  }

  private patchBufferAfterSave(
    path: string,
    writtenContent: string,
    note: VaultNoteContentResponse["note"],
    ifMatchToken: string,
  ) {
    const host = this.#host;
    const key = normalizeVaultNotePath(path.trim()) || path.trim();
    const prior = this.buffers.get(key);
    if (host.isFocusedPath(key)) {
      this.buffers.set(key, {
        path: key,
        content: host.content,
        baselineContent: writtenContent,
        contentHash: ifMatchToken,
        title: note.title,
        dirty: host.dirty,
        contentRevision: host.contentRevision,
      });
    } else {
      const content = prior?.content ?? writtenContent;
      this.buffers.set(key, {
        path: key,
        content,
        baselineContent: writtenContent,
        contentHash: ifMatchToken,
        title: note.title,
        dirty: content !== writtenContent,
        contentRevision: prior?.contentRevision ?? 0,
      });
    }
    host.bumpNoteBuffers();
  }

  private async runSaveJob(path: string, job: NoteSaveJob): Promise<NoteSaveResult> {
    const host = this.#host;
    try {
      const response = await saveVaultNote(path, job.content, {
        contentHash: job.force ? undefined : (job.contentHash ?? undefined),
        sessionId: workshopSessionIdForVaultSave(path),
      });
      const written = response.content ?? job.content;
      const ifMatchToken = vaultIfMatchToken(response);

      if (host.isFocusedPath(path)) {
        host.contentHash = ifMatchToken;
        host.title = response.note.title;
        host.selectedKind = resolveKind(response.note.path, response.note.kind);
        host.wikilinksOut = response.note.wikilinks_out;
        host.noteTags = response.note.tags ?? [];
        if (host.content === job.content || host.content === written) {
          host.content = written;
          host.baselineContent = written;
          host.dirty = false;
        } else {
          host.baselineContent = written;
          host.dirty = true;
        }
      }

      this.patchBufferAfterSave(path, written, response.note, ifMatchToken);

      invalidateMedousaViewCache(path);
      invalidateTransclusionCache(path);
      this.markSaveEcho(path);
      host.scheduleNotesRefresh();
      if (host.isFocusedPath(path)) {
        void host.refreshBacklinks(path);
      }

      return {
        ok: true,
        contentHash: ifMatchToken,
        writtenContent: written,
      };
    } catch (err) {
      if (isVaultConflictError(err)) {
        if (host.isFocusedPath(path)) {
          host.saveStatus = "conflict";
          host.conflictMessage =
            "This note changed on disk. Reload the latest version or keep your edits.";
        }
        return {
          ok: false,
          conflict: true,
          error: "conflict",
        };
      }
      const message = err instanceof Error ? err.message : String(err);
      if (host.isFocusedPath(path)) {
        host.error = message;
      }
      return { ok: false, error: message };
    }
  }

  async save(options?: { force?: boolean; source?: "manual" | "autosave" }) {
    const host = this.#host;
    if (!host.selectedPath) return false;
    if (host.noteLoading && options?.source === "autosave") return false;
    if (!host.dirty && !options?.force) return true;
    if (host.proposalActive && !options?.force) return false;

    this.clearAutosaveTimer();
    host.saving = true;
    host.saveStatus = "saving";
    host.error = null;

    const pathSnapshot = host.selectedPath;
    const contentSnapshot = host.content;
    const loosePath = host.looseFilePath;

    try {
      if (loosePath) {
        await writeAbsoluteTextFile(loosePath, contentSnapshot);
        if (host.selectedPath !== pathSnapshot || host.looseFilePath !== loosePath) {
          return true;
        }
        if (host.content !== contentSnapshot) {
          host.baselineContent = contentSnapshot;
          host.dirty = true;
          host.saveStatus = "unsaved";
          this.scheduleAutosave();
          return true;
        }
        host.baselineContent = host.content;
        host.dirty = false;
        this.clearProposal();
        this.flashSavedWhisper();
        return true;
      }

      const result = await this.#saveQueue.enqueue(pathSnapshot, {
        content: contentSnapshot,
        contentHash: options?.force ? null : host.contentHash,
        force: Boolean(options?.force),
        source: options?.source ?? "manual",
      });

      if (host.selectedPath !== pathSnapshot) return result.ok;

      if (!result.ok) {
        if (!result.conflict) {
          host.saveStatus = "unsaved";
          this.scheduleAutosave();
        }
        return false;
      }

      if (host.dirty) {
        host.saveStatus = "unsaved";
        this.scheduleAutosave();
        return true;
      }

      this.clearProposal();
      this.flashSavedWhisper();
      return true;
    } catch (err) {
      host.saveStatus = "unsaved";
      host.error = err instanceof Error ? err.message : String(err);
      this.scheduleAutosave();
      return false;
    } finally {
      host.saving = host.selectedPath
        ? this.#saveQueue.isBusy(host.selectedPath)
        : false;
    }
  }

  async flushSave() {
    await invokeVaultLeaveFlush();
    return this.save({ source: "manual" });
  }

  async reloadFromServer() {
    const host = this.#host;
    if (!host.selectedPath) return;
    if (host.looseFilePath) {
      host.noteLoading = true;
      host.error = null;
      try {
        const content = await readAbsoluteTextFile(host.looseFilePath);
        host.content = content;
        host.baselineContent = content;
        host.dirty = false;
        this.resetSaveState();
        this.clearProposal();
        host.bumpContentSync();
      } catch (err) {
        host.error = err instanceof Error ? err.message : String(err);
      } finally {
        host.noteLoading = false;
      }
      return;
    }
    host.noteLoading = true;
    host.error = null;
    try {
      const response = await getVaultNote(host.selectedPath);
      host.applyNote(response);
    } catch (err) {
      host.error = err instanceof Error ? err.message : String(err);
    } finally {
      host.noteLoading = false;
    }
  }

  async keepMineAndSave() {
    if (!this.#host.selectedPath) return false;
    const ok = await this.save({ force: true, source: "manual" });
    if (ok) {
      this.#host.conflictMessage = null;
    }
    return ok;
  }
}
