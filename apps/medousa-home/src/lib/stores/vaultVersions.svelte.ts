import {
  vaultGitCommit,
  vaultGitDetect,
  vaultGitDiff,
  vaultGitEnable,
  vaultGitInit,
  vaultGitInstall,
  vaultGitLog,
  vaultGitRestore,
  vaultGitStatus,
  vaultGitWorktrees,
  type VaultGitDetect,
  type VaultGitLogEntry,
  type VaultGitStatus,
} from "$lib/daemon";
import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";

class VaultVersionsStore {
  status = $state<VaultGitStatus | null>(null);
  detect = $state<VaultGitDetect | null>(null);
  history = $state<VaultGitLogEntry[]>([]);
  panelOpen = $state(false);
  advancedOpen = $state(false);
  busy = $state(false);
  error = $state<string | null>(null);
  lastDiff = $state<{ path: string; patch: string } | null>(null);
  worktrees = $state<Array<{ path: string; head: string; branch?: string | null }>>(
    [],
  );

  get enabled() {
    return (
      this.status?.enabled ??
      workshopDefaults.draft.vaultGitEnabled ??
      false
    );
  }

  async refresh() {
    try {
      this.detect = await vaultGitDetect();
      this.status = await vaultGitStatus();
      this.error = null;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  async setEnabled(enabled: boolean, initIfNeeded = true) {
    this.busy = true;
    this.error = null;
    try {
      workshopDefaults.draft = {
        ...workshopDefaults.draft,
        vaultGitEnabled: enabled,
      };
      await workshopDefaults.save();
      const result = await vaultGitEnable(enabled, initIfNeeded);
      this.status = result.status;
      this.detect = await vaultGitDetect();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.busy = false;
    }
  }

  async installGit() {
    this.busy = true;
    this.error = null;
    try {
      this.detect = await vaultGitInstall();
      this.status = await vaultGitStatus();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.busy = false;
    }
  }

  async startVersioning() {
    this.busy = true;
    this.error = null;
    try {
      this.status = await vaultGitInit();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.busy = false;
    }
  }

  async loadHistory(path?: string) {
    this.busy = true;
    this.error = null;
    try {
      this.history = await vaultGitLog({ path, limit: 40 });
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.busy = false;
    }
  }

  async saveVersion(message: string, paths?: string[]) {
    this.busy = true;
    this.error = null;
    try {
      await vaultGitCommit(message, paths);
      await this.refresh();
      if (this.panelOpen) {
        await this.loadHistory(paths?.[0]);
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.busy = false;
    }
  }

  async restore(commit: string, path: string) {
    this.busy = true;
    this.error = null;
    try {
      await vaultGitRestore(commit, path);
      await this.refresh();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.busy = false;
    }
  }

  async loadDiff(path: string, commit?: string) {
    this.busy = true;
    this.error = null;
    try {
      this.lastDiff = await vaultGitDiff(path, commit);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.busy = false;
    }
  }

  async loadWorktrees() {
    try {
      this.worktrees = await vaultGitWorktrees();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
  }

  openPanel() {
    this.panelOpen = true;
  }

  closePanel() {
    this.panelOpen = false;
  }
}

export const vaultVersions = new VaultVersionsStore();
