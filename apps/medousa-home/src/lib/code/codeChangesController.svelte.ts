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

export type CodeChangesLease = { leaseId: string; generation: number };

export type CodeChangesControllerDeps = {
  getWorkId: () => string;
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

  constructor(deps: CodeChangesControllerDeps) {
    this.#deps = deps;
  }

  restoreOpen(open: boolean) {
    this.open = open;
  }

  async refresh() {
    const workId = this.#deps.getWorkId();
    if (!workId || !this.open) return;
    this.loading = true;
    this.error = null;
    try {
      this.changes = await getForgeChanges(workId);
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
      if (isMissingForgeRoute(err)) {
        this.error = "This workshop does not expose Changes yet — update the daemon.";
      } else {
        this.error = err instanceof Error ? err.message : String(err);
      }
    } finally {
      this.loading = false;
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
    if (!workId) return;
    this.fileLoading = true;
    this.fileError = null;
    try {
      this.fileDiff = await getChangesFile(workId, path);
    } catch (err) {
      this.fileDiff = null;
      if (isMissingForgeRoute(err)) {
        this.fileError =
          "This workshop does not expose Changes diffs yet — update the daemon.";
      } else {
        this.fileError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      this.fileLoading = false;
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
    if (!workId) return;
    this.restoreBusy = true;
    try {
      const lease = await this.#deps.ensureLease();
      await restoreChangesFile(workId, {
        path: diff.path,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      await this.refresh();
      this.#deps.onFilesMutated();
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      this.restoreBusy = false;
    }
  }

  async revertHunk(diff: ChangesFileDiff, hunkIndex: number) {
    const workId = this.#deps.getWorkId();
    if (!workId || !diff.working_digest) return;
    this.restoreBusy = true;
    try {
      const lease = await this.#deps.ensureLease();
      await revertChangesHunk(workId, {
        path: diff.path,
        hunk_index: hunkIndex,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      await this.refresh();
      this.#deps.onFilesMutated();
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      this.restoreBusy = false;
    }
  }

  async resolveConflict(
    diff: ChangesFileDiff,
    resolution: "ours" | "theirs" | "baseline",
  ) {
    const workId = this.#deps.getWorkId();
    if (!workId) return;
    this.restoreBusy = true;
    try {
      const lease = await this.#deps.ensureLease();
      const result = await resolveChangesConflict(workId, {
        path: diff.path,
        resolution,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      this.changes = result.changes;
      await this.loadFileDiff(diff.path);
      this.#deps.onFilesMutated();
      this.syncMessage = `Conflict resolved (${resolution})`;
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      this.restoreBusy = false;
    }
  }

  async runSync(action: "fetch" | "pull" | "push" | "sync") {
    const workId = this.#deps.getWorkId();
    if (!workId) return;
    this.syncBusy = true;
    this.syncMessage = null;
    try {
      const lease = await this.#deps.ensureLease();
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
      this.changes = result.changes;
      this.syncMessage = result.message;
      if (this.selectedPath) await this.loadFileDiff(this.selectedPath);
      this.#deps.onFilesMutated();
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      this.syncBusy = false;
    }
  }

  async sealForReview() {
    const workId = this.#deps.getWorkId();
    if (!workId) return;
    this.syncBusy = true;
    try {
      const lease = await this.#deps.ensureLease();
      await checkpointChanges(workId, {
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      await this.#deps.refreshDetail();
      await this.#deps.openReview(
        workId,
        `Review · ${this.#deps.getReviewTitle()}`,
      );
      this.syncMessage = "Sealed for Review";
      await this.refresh();
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      this.syncBusy = false;
    }
  }

  async toggleHistory() {
    this.historyOpen = !this.historyOpen;
    const workId = this.#deps.getWorkId();
    if (!this.historyOpen || !workId) return;
    try {
      const result = await getChangesHistory(workId, 40);
      this.history = result.commits;
    } catch (err) {
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
    if (!workId || !this.selectedPath) return;
    this.blameHunks = null;
    try {
      const result = await getChangesBlame(workId, this.selectedPath);
      this.blameHunks = result.hunks;
    } catch (err) {
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
    if (this.#refreshTimer) clearTimeout(this.#refreshTimer);
    this.#refreshTimer = null;
  }
}
