/**
 * Forge Changes panel mode: list, file diff, sync, history, and blame.
 * CodeSourceEditor wires layout; this owns the panel state and actions.
 */

import {
  checkpointChanges,
  fetchChanges,
  getChangesBlame,
  getChangesFile,
  getChangesHistory,
  getForgeChanges,
  isMissingForgeRoute,
  pullChanges,
  pushChanges,
  resolveChangesConflict,
  restoreChangesFile,
  revertChangesHunk,
  syncChanges,
  type ChangesBlameHunk,
  type ChangesFileDiff,
  type ChangesHistoryEntry,
  type ForgeChanges,
} from "$lib/code/codeDocumentService";

import { captureCodeScope } from "$lib/code/codeWorkspaceContext.svelte";

export type CodeChangesLease = { leaseId: string; generation: number };

export type CodeChangesControllerDeps = {
  getWorkId: () => string;
  getScopeKey: () => string;
  persistOpen: (open: boolean) => void;
  ensureLease: () => Promise<CodeChangesLease>;
  onError: (message: string) => void;
  onFilesMutated: () => void;
  refreshDetail: () => Promise<void>;
  openReview: (workId: string, title: string) => Promise<void>;
  getReviewTitle: () => string;
};

export class CodeChangesController {
  open = $state(false);
  changes = $state<ForgeChanges | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  selectedPath = $state<string | null>(null);
  fileDiff = $state<ChangesFileDiff | null>(null);
  fileLoading = $state(false);
  fileError = $state<string | null>(null);
  restoreBusy = $state(false);
  syncBusy = $state(false);
  syncMessage = $state<string | null>(null);
  history = $state<ChangesHistoryEntry[]>([]);
  historyOpen = $state(false);
  blameOpen = $state(false);
  blameHunks = $state<ChangesBlameHunk[] | null>(null);

  #refreshTimer: ReturnType<typeof setTimeout> | null = null;
  #deps: CodeChangesControllerDeps;
  #epoch = 0;
  #diffEpoch = 0;

  constructor(deps: CodeChangesControllerDeps) {
    this.#deps = deps;
  }

  #captureScope() {
    const epoch = this.#epoch;
    const current = captureCodeScope(this.#deps.getScopeKey);
    return () => epoch === this.#epoch && current();
  }

  resetForScope() {
    this.dispose();
    this.changes = null;
    this.loading = false;
    this.error = null;
    this.selectedPath = null;
    this.fileDiff = null;
    this.fileLoading = false;
    this.fileError = null;
    this.restoreBusy = false;
    this.syncBusy = false;
    this.syncMessage = null;
    this.history = [];
    this.historyOpen = false;
    this.blameOpen = false;
    this.blameHunks = null;
  }

  restoreOpen(open: boolean) {
    this.open = open;
  }

  async refresh() {
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!workId || !this.open) return;
    this.loading = true;
    this.error = null;
    try {
      const changes = await getForgeChanges(workId);
      if (!current()) return;
      this.changes = changes;
      if (
        this.selectedPath &&
        !this.changes.files.some((file) => file.path === this.selectedPath)
      ) {
        this.selectedPath = null;
        this.fileDiff = null;
        this.fileError = null;
      } else if (this.selectedPath) {
        await this.loadFileDiff(this.selectedPath);
      }
    } catch (err) {
      if (!current()) return;
      if (isMissingForgeRoute(err)) {
        this.error = "This workshop does not expose Changes yet — update the daemon.";
      } else {
        this.error = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (current()) this.loading = false;
    }
  }

  scheduleRefresh() {
    if (!this.open || !this.#deps.getWorkId()) return;
    if (this.#refreshTimer) clearTimeout(this.#refreshTimer);
    this.#refreshTimer = setTimeout(() => {
      this.#refreshTimer = null;
      void this.refresh();
    }, 200);
  }

  async loadFileDiff(path: string) {
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!workId) return;
    const diffEpoch = ++this.#diffEpoch;
    const currentDiff = () => current() && diffEpoch === this.#diffEpoch;
    this.fileLoading = true;
    this.fileError = null;
    try {
      const diff = await getChangesFile(workId, path);
      if (!currentDiff()) return;
      this.fileDiff = diff;
    } catch (err) {
      if (!currentDiff()) return;
      this.fileDiff = null;
      if (isMissingForgeRoute(err)) {
        this.fileError =
          "This workshop does not expose Changes diffs yet — update the daemon.";
      } else {
        this.fileError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (currentDiff()) this.fileLoading = false;
    }
  }

  async selectPath(path: string) {
    this.selectedPath = path;
    this.blameOpen = false;
    this.blameHunks = null;
    await this.loadFileDiff(path);
  }

  async restoreFile(diff: ChangesFileDiff) {
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!workId) return;
    this.restoreBusy = true;
    try {
      const lease = await this.#deps.ensureLease();
      if (!current()) return;
      await restoreChangesFile(workId, {
        path: diff.path,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      if (!current()) return;
      await this.refresh();
      if (!current()) return;
      this.#deps.onFilesMutated();
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      if (current()) this.restoreBusy = false;
    }
  }

  async revertHunk(diff: ChangesFileDiff, hunkIndex: number) {
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!workId || !diff.working_digest) return;
    this.restoreBusy = true;
    try {
      const lease = await this.#deps.ensureLease();
      if (!current()) return;
      await revertChangesHunk(workId, {
        path: diff.path,
        hunk_index: hunkIndex,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      if (!current()) return;
      await this.refresh();
      if (!current()) return;
      this.#deps.onFilesMutated();
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      if (current()) this.restoreBusy = false;
    }
  }

  async resolveConflict(
    diff: ChangesFileDiff,
    resolution: "ours" | "theirs" | "baseline",
  ) {
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!workId) return;
    this.restoreBusy = true;
    try {
      const lease = await this.#deps.ensureLease();
      if (!current()) return;
      const result = await resolveChangesConflict(workId, {
        path: diff.path,
        resolution,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      if (!current()) return;
      this.changes = result.changes;
      await this.loadFileDiff(diff.path);
      if (!current()) return;
      this.#deps.onFilesMutated();
      this.syncMessage = `Conflict resolved (${resolution})`;
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      if (current()) this.restoreBusy = false;
    }
  }

  async runSync(action: "fetch" | "pull" | "push" | "sync") {
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!workId) return;
    this.syncBusy = true;
    this.syncMessage = null;
    try {
      const lease = await this.#deps.ensureLease();
      if (!current()) return;
      const body = {
        lease_id: lease.leaseId,
        generation: lease.generation,
      };
      const result =
        action === "fetch"
          ? await fetchChanges(workId, body)
          : action === "pull"
            ? await pullChanges(workId, body)
            : action === "push"
              ? await pushChanges(workId, body)
              : await syncChanges(workId, body);
      if (!current()) return;
      this.changes = result.changes;
      this.syncMessage = result.message;
      if (this.selectedPath) await this.loadFileDiff(this.selectedPath);
      if (!current()) return;
      this.#deps.onFilesMutated();
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      if (current()) this.syncBusy = false;
    }
  }

  async sealForReview() {
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!workId) return;
    this.syncBusy = true;
    try {
      const lease = await this.#deps.ensureLease();
      if (!current()) return;
      await checkpointChanges(workId, {
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      if (!current()) return;
      await this.#deps.refreshDetail();
      if (!current()) return;
      await this.#deps.openReview(
        workId,
        `Review · ${this.#deps.getReviewTitle()}`,
      );
      if (!current()) return;
      this.syncMessage = "Sealed for Review";
      await this.refresh();
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      if (current()) this.syncBusy = false;
    }
  }

  async toggleHistory() {
    this.historyOpen = !this.historyOpen;
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!this.historyOpen || !workId) return;
    try {
      const result = await getChangesHistory(workId, 40);
      if (current()) this.history = result.commits;
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    }
  }

  async toggleBlame() {
    this.blameOpen = !this.blameOpen;
    if (!this.blameOpen) {
      this.blameHunks = null;
      return;
    }
    const workId = this.#deps.getWorkId();
    const current = this.#captureScope();
    if (!workId || !this.selectedPath) return;
    this.blameHunks = null;
    const path = this.selectedPath;
    try {
      const result = await getChangesBlame(workId, path);
      if (current() && this.selectedPath === path) this.blameHunks = result.hunks;
    } catch (err) {
      if (!current()) return;
      this.#deps.onError(err instanceof Error ? err.message : String(err));
      this.blameOpen = false;
    }
  }

  async toggle(forceOpen?: boolean) {
    const next =
      forceOpen === true ? true : forceOpen === false ? false : !this.open;
    this.open = next;
    this.#deps.persistOpen(next);
    if (next) await this.refresh();
  }

  dispose() {
    this.#epoch += 1;
    this.#diffEpoch += 1;
    if (this.#refreshTimer) clearTimeout(this.#refreshTimer);
    this.#refreshTimer = null;
  }
}
