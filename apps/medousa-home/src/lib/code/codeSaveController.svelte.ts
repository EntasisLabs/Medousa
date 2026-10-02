/**
 * Human edit / save / agent handoff for the Code editor.
 * CodeSourceEditor wires the chrome; this owns busy flags and save actions.
 */

import {
  canStartHumanEditing,
  humanizeForgeMessage,
  saveUndertakingSource,
  startHumanEditingSession,
  type ForgeSourceFile,
} from "$lib/code/codeDocumentService";
import {
  CODE_SAVE_NO_LEASE_ERROR,
  CODE_SAVE_PREVIEW_ERROR,
  decideCodeSave,
} from "$lib/code/codeSaveGate";

import { captureCodeScope } from "$lib/code/codeWorkspaceContext.svelte";

export type CodeSaveTab = {
  tabId: string;
  path: string;
  draft: string;
  digest: string;
  preview?: boolean;
};

export type CodeSaveLease = { leaseId: string; generation: number };

export type CodeSaveEditorHost = {
  getValue: () => string;
  flushChanges: () => void;
};

export type CodeSaveControllerDeps = {
  getWorkId: () => string;
  getScopeKey: () => string;
  getContext: () => {
    workId?: string;
    leaseId?: string | null;
    leaseGeneration?: number | null;
  } | null;
  getDetail: () => {
    id: string;
    allowed_actions: Parameters<typeof canStartHumanEditing>[0];
  } | null;
  getActiveTab: () => CodeSaveTab | null;
  getActiveTabId: () => string;
  getTabs: () => CodeSaveTab[];
  getEditor: () => CodeSaveEditorHost | undefined;
  getEditable: () => boolean;
  getCanBeginEdit: () => boolean;
  ensureLease: () => Promise<CodeSaveLease>;
  onError: (message: string | null) => void;
  captureEditorContext: () => void;
  preferredAgent: () => "codex" | "cursor" | "hermes";
  onHandoffToAgent?: (runtime: "codex" | "cursor" | "hermes", draft?: string) => Promise<void>;
  onReclaimHuman?: () => Promise<void>;
  updateDraft: (tabId: string, value: string) => void;
  isDirty: (tab: CodeSaveTab) => boolean;
  acceptSaved: (tabId: string, source: ForgeSourceFile) => void;
  setTabError: (tabId: string, message: string | null) => void;
  setActiveFromItem: (
    item: Awaited<ReturnType<typeof startHumanEditingSession>>["item"],
    lease: { leaseId: string; leaseGeneration: number; executorKind: "human" },
  ) => void;
  refreshDetail: () => Promise<void>;
  /** Optional active-document transform (for example format-on-save). */
  beforeSave?: (tab: CodeSaveTab) => Promise<boolean>;
};

export class CodeSaveController {
  savingFile = $state(false);
  beginningEdit = $state(false);
  handingOff = $state(false);
  saveWhisper = $state<string | null>(null);
  beginEditPromise = $state<Promise<void> | null>(null);
  #whisperTimer: ReturnType<typeof setTimeout> | null = null;
  #deps: CodeSaveControllerDeps;
  #epoch = 0;

  constructor(deps: CodeSaveControllerDeps) {
    this.#deps = deps;
  }

  #captureScope() {
    const epoch = this.#epoch;
    const current = captureCodeScope(this.#deps.getScopeKey);
    return () => epoch === this.#epoch && current();
  }

  resetForScope() {
    this.dispose();
    this.savingFile = false;
    this.beginningEdit = false;
    this.handingOff = false;
    this.saveWhisper = null;
    this.beginEditPromise = null;
  }

  get busy(): boolean {
    return this.savingFile || this.beginningEdit || this.handingOff;
  }

  async startEditing() {
    const current = this.#captureScope();
    const detail = this.#deps.getDetail();
    if (!detail || detail.id !== this.#deps.getWorkId()) return;
    const allowedActions = detail.allowed_actions;
    if (!allowedActions || !canStartHumanEditing(allowedActions)) return;
    if (this.beginEditPromise) {
      await this.beginEditPromise;
      return;
    }
    this.beginningEdit = true;
    this.#deps.onError(null);
    this.beginEditPromise = (async () => {
      const begun = await startHumanEditingSession(detail.id, allowedActions);
      if (!current()) return;
      this.#deps.setActiveFromItem(begun.item, {
        leaseId: begun.lease.lease_id,
        leaseGeneration: begun.lease.generation,
        executorKind: "human",
      });
      await this.#deps.refreshDetail();
    })();
    try {
      await this.beginEditPromise;
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(
        humanizeForgeMessage(err instanceof Error ? err.message : String(err)),
      );
    } finally {
      if (current()) {
        this.beginEditPromise = null;
        this.beginningEdit = false;
      }
    }
  }

  async onDraftChanged(tabIdValue: string, value: string) {
    this.#deps.updateDraft(tabIdValue, value);
    if (!this.#deps.getEditable() && this.#deps.getCanBeginEdit()) {
      await this.startEditing();
    }
  }

  async saveTab(tab: CodeSaveTab | null): Promise<boolean> {
    const current = this.#captureScope();
    if (!tab) return true;
    if (tab.preview) {
      this.#deps.onError(CODE_SAVE_PREVIEW_ERROR);
      return false;
    }
    const editor = this.#deps.getEditor();
    if (tab.tabId === this.#deps.getActiveTabId() && editor) {
      const liveDraft = editor.getValue();
      if (liveDraft !== tab.draft) {
        this.#deps.updateDraft(tab.tabId, liveDraft);
        tab = { ...tab, draft: liveDraft };
      }
      editor.flushChanges();
    }

    const context = this.#deps.getContext();
    const workId = this.#deps.getWorkId();
    const decision = decideCodeSave({
      preview: Boolean(tab.preview),
      dirty: this.#deps.isDirty(tab),
      savingFile: this.savingFile,
      hasLease: Boolean(
        context?.workId === workId &&
          context.leaseId &&
          context.leaseGeneration != null,
      ),
      canBeginEdit: this.#deps.getCanBeginEdit(),
      beginningEdit: this.beginningEdit || Boolean(this.beginEditPromise),
    });

    if (decision.action === "noop") {
      return decision.reason === "not-dirty" || decision.reason === "already-saving";
    }
    if (decision.action === "reject") {
      this.#deps.onError(
        decision.reason === "preview" ? CODE_SAVE_PREVIEW_ERROR : CODE_SAVE_NO_LEASE_ERROR,
      );
      return false;
    }
    if (decision.action === "await-lease") {
      if (this.beginEditPromise) {
        try {
          await this.beginEditPromise;
        } catch (err) {
          if (!current()) return false;
          this.#deps.onError(
            humanizeForgeMessage(err instanceof Error ? err.message : String(err)),
          );
          return false;
        }
      }
    } else if (decision.action === "begin-then-save") {
      try {
        await this.startEditing();
      } catch (err) {
        if (!current()) return false;
        this.#deps.onError(
          humanizeForgeMessage(err instanceof Error ? err.message : String(err)),
        );
        return false;
      }
    }

    if (!current()) return false;
    let leaseId = context?.workId === workId ? context.leaseId ?? null : null;
    let generation = context?.workId === workId ? context.leaseGeneration ?? null : null;
    if (!leaseId || generation == null) {
      try {
        const lease = await this.#deps.ensureLease();
        if (!current()) return false;
        leaseId = lease.leaseId;
        generation = lease.generation;
      } catch (err) {
        if (!current()) return false;
        this.#deps.onError(
          humanizeForgeMessage(err instanceof Error ? err.message : String(err)),
        );
        return false;
      }
    }
    if (!leaseId || generation == null || !this.#deps.isDirty(tab) || this.savingFile) {
      return !this.#deps.isDirty(tab);
    }

    if (this.#deps.beforeSave && !(await this.#deps.beforeSave(tab))) return false;
    if (!current()) return false;
    if (tab.tabId === this.#deps.getActiveTabId() && editor) {
      const transformedDraft = editor.getValue();
      if (transformedDraft !== tab.draft) {
        this.#deps.updateDraft(tab.tabId, transformedDraft);
        tab = { ...tab, draft: transformedDraft };
      }
      editor.flushChanges();
    }

    this.savingFile = true;
    this.saveWhisper = "Saving…";
    if (this.#whisperTimer) clearTimeout(this.#whisperTimer);
    this.#deps.onError(null);
    this.#deps.setTabError(tab.tabId, null);
    try {
      const next = await saveUndertakingSource(workId, {
        path: tab.path,
        content: tab.draft,
        lease_id: leaseId,
        generation,
        expected_digest: tab.digest,
      });
      if (!current()) return false;
      this.#deps.acceptSaved(tab.tabId, next);
      this.saveWhisper = "Saved";
      this.#whisperTimer = setTimeout(() => {
        this.saveWhisper = null;
      }, 1600);
      return true;
    } catch (err) {
      if (!current()) return false;
      this.saveWhisper = null;
      const message = humanizeForgeMessage(
        err instanceof Error ? err.message : String(err),
      );
      this.#deps.setTabError(tab.tabId, message);
      this.#deps.onError(message);
      return false;
    } finally {
      if (current()) this.savingFile = false;
    }
  }

  async save() {
    const current = this.#captureScope();
    const activeTab = this.#deps.getActiveTab();
    const ok = await this.saveTab(activeTab);
    if (current() && !ok && activeTab && this.#deps.isDirty(activeTab)) {
      this.#deps.onError("Could not save the file.");
    }
  }

  async saveAll(): Promise<boolean> {
    const current = this.#captureScope();
    for (const tab of this.#deps.getTabs()) {
      if (!current()) return false;
      if (this.#deps.isDirty(tab) && !(await this.saveTab(tab))) return false;
    }
    return current();
  }

  async handoffToAgent(draft?: string) {
    const current = this.#captureScope();
    if (!this.#deps.onHandoffToAgent || this.busy) return;
    this.#deps.onError(null);
    const saved = await this.saveAll();
    if (!current()) return;
    if (!saved) {
      this.#deps.onError("Resolve the unsaved file before asking an agent to continue.");
      return;
    }
    this.handingOff = true;
    try {
      this.#deps.captureEditorContext();
      await this.#deps.onHandoffToAgent(this.#deps.preferredAgent(), draft);
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(
        humanizeForgeMessage(err instanceof Error ? err.message : String(err)),
      );
    } finally {
      if (current()) this.handingOff = false;
    }
  }

  async reclaimHuman() {
    const current = this.#captureScope();
    if (!this.#deps.onReclaimHuman || this.busy) return;
    this.handingOff = true;
    this.#deps.onError(null);
    try {
      await this.#deps.onReclaimHuman();
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(
        humanizeForgeMessage(err instanceof Error ? err.message : String(err)),
      );
    } finally {
      if (current()) this.handingOff = false;
    }
  }

  flashWhisper(message: string, ms = 1600) {
    this.saveWhisper = message;
    if (this.#whisperTimer) clearTimeout(this.#whisperTimer);
    this.#whisperTimer = setTimeout(() => {
      this.saveWhisper = null;
    }, ms);
  }

  dispose() {
    this.#epoch += 1;
    if (this.#whisperTimer) clearTimeout(this.#whisperTimer);
    this.#whisperTimer = null;
  }
}
