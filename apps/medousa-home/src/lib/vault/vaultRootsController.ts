import {
  addVaultRoot,
  listVaultRoots,
  setActiveVaultRoot,
} from "$lib/daemon";
import type { VaultNote, VaultRootView, VaultSearchHit, VaultTreeNode } from "$lib/types/vault";
import type { NoteBuffer } from "$lib/stores/noteBuffer";
import type { VaultNoteKind } from "$lib/utils/vaultFrontmatter";
import { noteEditorRuntimes } from "$lib/vault/noteEditorRuntimes.svelte";
import { toast } from "$lib/runtime/toast.svelte";
import { isAbsoluteDiskPath } from "$lib/utils/vaultNoteTitle";
import {
  fileNameFromAbsolutePath,
  invalidateVaultRootCache,
  pickMarkdownFile,
  readAbsoluteTextFile,
} from "$lib/utils/vaultFilesystem";

export type VaultRootsHost = {
  vaultRoots: VaultRootView[];
  activeVaultRootId: string | null;
  vaultRootsLoading: boolean;
  vaultRootsError: string | null;
  vaultRootsUnavailable: boolean;
  addVaultRootOpen: boolean;
  looseFilePath: string | null;
  selectedPath: string | null;
  content: string;
  baselineContent: string;
  contentHash: string | null;
  title: string;
  dirty: boolean;
  noteTags: string[];
  wikilinksOut: string[];
  backlinks: string[];
  searchHits: VaultSearchHit[];
  searchQuery: string;
  notes: VaultNote[];
  tree: VaultTreeNode[];
  error: string | null;
  noteLoading: boolean;
  loading: boolean;
  selectedKind: VaultNoteKind;
  openGeneration: number;
  clearAutosaveTimer(): void;
  clearProposal(): void;
  clearRailSelection(): void;
  flushBeforeLeave(options?: { skipEditorFlush?: boolean }): Promise<boolean>;
  restoreBufferIntoFocused(buffer: NoteBuffer): void;
  writeAbsoluteBuffer(path: string, content: string, title: string): void;
  deleteBuffer(path: string): void;
  getBuffer(path: string): NoteBuffer | undefined;
  resetSaveState(): void;
  closeAttachmentPreview(): void;
  restoreEditorUi(path: string): void;
  syncLmeNoteTab(path: string): Promise<void>;
  bumpContentSync(): void;
  bumpNoteBuffers(): void;
  refreshNotes(): Promise<void>;
  resetEditorBuffers(): void;
};

export class VaultRootsController {
  #host: VaultRootsHost;
  #looseOpenInFlight = new Map<string, Promise<boolean>>();

  constructor(host: VaultRootsHost) {
    this.#host = host;
  }

  clearLooseFile() {
    this.#host.looseFilePath = null;
  }

  clearLooseOpenInFlight() {
    this.#looseOpenInFlight.clear();
  }

  async openLooseMarkdownFile() {
    const path = await pickMarkdownFile();
    if (!path) return false;
    return this.openLooseFile(path);
  }

  async openLooseFile(
    absolutePath: string,
    options?: { skipLeaveFlush?: boolean },
  ) {
    const trimmed = absolutePath.trim();
    if (!trimmed || !isAbsoluteDiskPath(trimmed)) return false;
    const existingOpen = this.#looseOpenInFlight.get(trimmed);
    if (existingOpen) return existingOpen;
    const pending = this.performOpenLooseFile(trimmed, options).finally(() => {
      if (this.#looseOpenInFlight.get(trimmed) === pending) {
        this.#looseOpenInFlight.delete(trimmed);
      }
    });
    this.#looseOpenInFlight.set(trimmed, pending);
    return pending;
  }

  private async performOpenLooseFile(
    trimmed: string,
    options?: { skipLeaveFlush?: boolean },
  ) {
    const host = this.#host;
    if (
      host.looseFilePath === trimmed &&
      host.selectedPath === trimmed &&
      !host.noteLoading
    ) {
      const hasSession =
        host.contentHash != null ||
        host.dirty ||
        Boolean(host.getBuffer(trimmed)) ||
        Boolean(host.content.trim());
      if (hasSession) {
        noteEditorRuntimes.touch(trimmed);
        return true;
      }
    }

    const openGen = ++host.openGeneration;

    if (host.selectedPath && host.selectedPath !== trimmed) {
      const ok = options?.skipLeaveFlush
        ? await host.flushBeforeLeave({ skipEditorFlush: true })
        : await host.flushBeforeLeave();
      if (!ok) return false;
      if (openGen !== host.openGeneration) return false;
      host.clearProposal();
      host.closeAttachmentPreview();
    }
    if (openGen !== host.openGeneration) return false;

    const buffered = host.getBuffer(trimmed);
    if (buffered) {
      this.clearLooseFile();
      host.looseFilePath = trimmed;
      host.selectedPath = trimmed;
      host.resetSaveState();
      host.restoreBufferIntoFocused(buffered);
      host.wikilinksOut = [];
      host.backlinks = [];
      host.noteTags = [];
      host.selectedKind = "note";
      host.error = null;
      host.restoreEditorUi(trimmed);
      await host.syncLmeNoteTab(trimmed);
      return true;
    }

    host.noteLoading = true;
    host.loading = true;
    host.error = null;
    this.clearLooseFile();
    host.looseFilePath = trimmed;
    host.selectedPath = trimmed;
    host.resetSaveState();
    host.content = "";
    host.baselineContent = "";
    host.contentHash = null;
    host.title = fileNameFromAbsolutePath(trimmed)
      .replace(/\.md$/i, "")
      .replace(/\.markdown$/i, "") || trimmed;
    host.dirty = false;
    host.selectedKind = "note";
    host.bumpContentSync();
    await host.syncLmeNoteTab(trimmed);
    if (openGen !== host.openGeneration || host.looseFilePath !== trimmed) {
      return false;
    }
    try {
      const content = await readAbsoluteTextFile(trimmed);
      if (openGen !== host.openGeneration || host.looseFilePath !== trimmed) {
        return false;
      }
      const name = fileNameFromAbsolutePath(trimmed);
      const title = name.replace(/\.md$/i, "").replace(/\.markdown$/i, "") || name;
      host.content = content;
      host.baselineContent = content;
      host.title = title;
      host.wikilinksOut = [];
      host.backlinks = [];
      host.noteTags = [];
      host.dirty = false;
      host.writeAbsoluteBuffer(trimmed, content, title);
      host.restoreEditorUi(trimmed);
      host.bumpContentSync();
      return true;
    } catch (err) {
      if (openGen === host.openGeneration && host.looseFilePath === trimmed) {
        host.error = err instanceof Error ? err.message : String(err);
        host.content = "";
        host.baselineContent = "";
        host.contentHash = null;
        host.dirty = false;
        host.deleteBuffer(trimmed);
        host.bumpNoteBuffers();
        host.bumpContentSync();
      }
      return false;
    } finally {
      if (openGen === host.openGeneration) {
        host.noteLoading = false;
        host.loading = false;
      }
    }
  }

  async refreshVaultRoots() {
    const host = this.#host;
    host.vaultRootsLoading = true;
    host.vaultRootsError = null;
    try {
      const response = await listVaultRoots();
      host.vaultRootsUnavailable = false;
      host.vaultRoots = response.roots;
      host.activeVaultRootId = response.activeRootId;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (/404|not found/i.test(message)) {
        host.vaultRootsUnavailable = true;
        host.vaultRootsError = null;
        host.vaultRoots = [
          {
            id: "personal",
            label: "Personal",
            path: "",
            isDefault: true,
            active: true,
            isObsidian: false,
          },
        ];
        host.activeVaultRootId = "personal";
      } else {
        host.vaultRootsError = message;
      }
    } finally {
      host.vaultRootsLoading = false;
    }
  }

  async switchVaultRoot(rootId: string) {
    const host = this.#host;
    if (!rootId.trim() || rootId === host.activeVaultRootId) return;
    host.clearAutosaveTimer();
    host.clearProposal();
    this.clearLooseFile();
    host.selectedPath = null;
    host.clearRailSelection();
    host.content = "";
    host.baselineContent = "";
    host.contentHash = null;
    host.noteTags = [];
    host.wikilinksOut = [];
    host.backlinks = [];
    host.title = "";
    host.dirty = false;
    host.searchHits = [];
    host.searchQuery = "";
    host.notes = [];
    host.tree = [];
    host.error = null;
    invalidateVaultRootCache();
    try {
      const response = await setActiveVaultRoot(rootId);
      host.vaultRoots = response.roots;
      host.activeVaultRootId = response.activeRootId;
      await host.refreshNotes();
    } catch (err) {
      host.error = err instanceof Error ? err.message : String(err);
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
    this.#host.vaultRoots = response.roots;
    this.#host.activeVaultRootId = response.activeRootId;
    invalidateVaultRootCache();
  }

  openAddVaultRootDialog() {
    void import("$lib/utils/workshopLocality").then(
      ({ isCoLocatedWorkshop, vaultAddRootRemoteHint }) => {
        if (!isCoLocatedWorkshop()) {
          toast.show(vaultAddRootRemoteHint());
          return;
        }
        this.#host.addVaultRootOpen = true;
      },
    );
  }

  closeAddVaultRootDialog() {
    this.#host.addVaultRootOpen = false;
  }

  resetForWorkshopSwitch() {
    const host = this.#host;
    host.clearAutosaveTimer();
    host.resetEditorBuffers();
    this.clearLooseOpenInFlight();
    noteEditorRuntimes.resetForWorkshopSwitch();
    host.clearProposal();
    this.clearLooseFile();
    host.selectedPath = null;
    host.content = "";
    host.baselineContent = "";
    host.contentHash = null;
    host.noteTags = [];
    host.wikilinksOut = [];
    host.backlinks = [];
    host.title = "";
    host.dirty = false;
    host.searchHits = [];
    host.searchQuery = "";
    host.notes = [];
    host.tree = [];
    host.error = null;
    host.vaultRoots = [];
    host.activeVaultRootId = null;
    host.vaultRootsUnavailable = false;
    invalidateVaultRootCache();
    void import("$lib/utils/vaultLocalImages").then(({ clearDaemonImagePreviewCache }) => {
      clearDaemonImagePreviewCache();
    });
    void this.refreshVaultRoots();
    void host.refreshNotes();
  }
}
