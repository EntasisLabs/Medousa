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
import { activeWorkshopId } from "$lib/utils/workshopLocality";
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
  workshopEpoch: number;
  saving: boolean;
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
  refreshNotesForCurrentRoot(): Promise<void>;
  flushForVaultRootChange(): Promise<boolean>;
  hasPendingVaultEdits(): boolean;
  resetForVaultRootChange(): Promise<void>;
  restoreVaultRootPreferences(): void;
  resetEditorBuffers(): void;
};

export class VaultRootsController {
  #host: VaultRootsHost;
  #looseOpenInFlight = new Map<string, Promise<boolean>>();
  #initializedEpoch = -1;
  #refresh: { epoch: number; work: Promise<void> } | null = null;
  #switching: Promise<void> | null = null;

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

  async ensureVaultRoots() {
    if (this.#switching) await this.#switching;
    if (this.#initializedEpoch === this.#host.workshopEpoch) return;
    await this.loadVaultRoots();
    if (this.#initializedEpoch !== this.#host.workshopEpoch) throw new Error(this.#host.vaultRootsError ?? "Couldn’t determine the active vault. Try again.");
  }
  async refreshNotes() {
    try { await this.refreshVaultRoots(); await this.ensureVaultRoots(); await this.#host.refreshNotesForCurrentRoot(); }
    catch (cause) { this.#host.error = cause instanceof Error ? cause.message : String(cause); }
  }
  async prepareForNoteAccess() {
    const selected = this.#host.activeVaultRootId;
    await this.refreshVaultRoots(); await this.ensureVaultRoots();
    if (selected && selected !== this.#host.activeVaultRootId) throw new Error("The active vault changed. Open the note again from its current vault.");
  }

  async refreshVaultRoots() {
    if (this.#switching) await this.#switching;
    await this.loadVaultRoots();
  }

  private loadVaultRoots(): Promise<void> {
    const epoch = this.#host.workshopEpoch;
    if (this.#refresh?.epoch === epoch) return this.#refresh.work;
    const work = this.performRefreshVaultRoots().finally(() => {
      if (this.#refresh?.work === work) this.#refresh = null;
    });
    this.#refresh = { epoch, work };
    return work;
  }

  private async performRefreshVaultRoots() {
    const host = this.#host;
    let workshopEpoch = host.workshopEpoch;
    host.vaultRootsLoading = true;
    host.vaultRootsError = null;
    try {
      const response = await listVaultRoots();
      if (workshopEpoch !== host.workshopEpoch) return;
      if (host.activeVaultRootId && host.activeVaultRootId !== response.activeRootId) {
        if (host.hasPendingVaultEdits()) throw new Error("The active vault changed elsewhere. Switch back to save your pending note edits.");
        await host.resetForVaultRootChange();
        workshopEpoch += 1;
        if (workshopEpoch !== host.workshopEpoch) return;
      }
      host.vaultRootsUnavailable = false;
      host.vaultRoots = response.roots;
      host.activeVaultRootId = response.activeRootId;
      host.restoreVaultRootPreferences();
      this.#initializedEpoch = host.workshopEpoch;
    } catch (err) {
      if (workshopEpoch !== host.workshopEpoch) return;
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
        this.#initializedEpoch = host.workshopEpoch;
      } else {
        this.#initializedEpoch = -1;
        host.vaultRootsError = message;
      }
    } finally {
      if (workshopEpoch === host.workshopEpoch) {
        host.vaultRootsLoading = false;
      }
    }
  }

  switchVaultRoot(rootId: string): Promise<void> {
    const prior = this.#switching;
    const scope = activeWorkshopId();
    const work = (prior ? prior.catch(() => {}) : Promise.resolve()).then(() => this.performSwitchVaultRoot(rootId, scope));
    this.#switching = work;
    void work.finally(() => { if (this.#switching === work) this.#switching = null; }).catch(() => {});
    return work;
  }

  private async performSwitchVaultRoot(rootId: string, scope: string) {
    const host = this.#host;
    if (scope !== activeWorkshopId()) throw new Error("The workshop changed before switching vaults.");
    if (!rootId.trim() || rootId === host.activeVaultRootId) return;
    const epoch = host.workshopEpoch;
    if (!(await host.flushForVaultRootChange())) {
      host.error ??= "Save your pending note edits before switching vaults.";
      throw new Error(host.error);
    }
    if (epoch !== host.workshopEpoch || scope !== activeWorkshopId()) return;
    if (host.saving) throw new Error("Wait for the current note operation before switching vaults.");
    host.clearAutosaveTimer();
    host.openGeneration += 1;
    host.noteLoading = true;
    try {
      const response = await setActiveVaultRoot(rootId);
      if (epoch !== host.workshopEpoch || scope !== activeWorkshopId()) return;
      await host.resetForVaultRootChange();
      if (host.workshopEpoch !== epoch + 1 || scope !== activeWorkshopId()) return;
      host.vaultRoots = response.roots;
      host.activeVaultRootId = response.activeRootId;
      host.restoreVaultRootPreferences();
      this.#initializedEpoch = host.workshopEpoch;
      await host.refreshNotesForCurrentRoot();
    } catch (err) {
      if (scope === activeWorkshopId() && (host.workshopEpoch === epoch || host.workshopEpoch === epoch + 1)) host.error = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      if (scope === activeWorkshopId() && (host.workshopEpoch === epoch || host.workshopEpoch === epoch + 1)) host.noteLoading = false;
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

  resetForWorkshopSwitch(preserveSwitch = false) {
    this.#initializedEpoch = -1;
    if (!preserveSwitch) { this.#switching = null; this.#refresh = null; }
    const host = this.#host;
    host.clearAutosaveTimer();
    host.resetEditorBuffers();
    this.clearLooseOpenInFlight();
    noteEditorRuntimes.resetForWorkshopSwitch();
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
    host.noteLoading = false;
    host.loading = false;
    host.vaultRoots = [];
    host.activeVaultRootId = null;
    host.vaultRootsLoading = false;
    host.vaultRootsError = null;
    host.vaultRootsUnavailable = false;
    invalidateVaultRootCache();
    void import("$lib/utils/vaultLocalImages").then(({ clearDaemonImagePreviewCache }) => {
      clearDaemonImagePreviewCache();
    });
  }
}
