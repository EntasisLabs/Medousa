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
import {
  friendlySettingsError,
  isMissingCapabilityError,
} from "$lib/utils/normieErrors";

class VaultVersionsStore {
  status = $state<VaultGitStatus | null>(null);
  detect = $state<VaultGitDetect | null>(null);
  history = $state<VaultGitLogEntry[]>([]);
  panelOpen = $state(false);
  advancedOpen = $state(false);
  busy = $state(false);
  error = $state<string | null>(null);
  /** Older engines without vault-git routes. */
  unsupported = $state(false);
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

  /** Local off-state — no engine probe while Versions is disabled. */
  markDisabledLocally() {
    this.error = null;
    // Idempotent: avoid churning $state when already disabled.
    if (this.status && !this.status.enabled && !this.status.isRepo) {
      return;
    }
    const detect = this.detect;
    const vaultRoot = this.status?.vaultRoot ?? "";
    this.status = {
      enabled: false,
      available: detect?.available ?? false,
      isRepo: false,
      dirtyCount: 0,
      vaultRoot,
      branch: null,
      gitPath: detect?.path ?? null,
    };
  }

  async refresh(options?: { force?: boolean }) {
    const enabled =
      workshopDefaults.draft.vaultGitEnabled ?? this.status?.enabled ?? false;
    if (!enabled && !options?.force) {
      this.markDisabledLocally();
      return;
    }
    try {
      this.detect = await vaultGitDetect();
      this.status = await vaultGitStatus();
      this.unsupported = false;
      this.error = null;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (isMissingCapabilityError(message)) {
        this.unsupported = true;
        this.error = null;
        if (enabled) {
          workshopDefaults.draft = {
            ...workshopDefaults.draft,
            vaultGitEnabled: false,
          };
        }
        this.markDisabledLocally();
        this.unsupported = true;
        return;
      }
      this.error = friendlySettingsError(message, "Versions");
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
      if (!enabled) {
        this.markDisabledLocally();
        // Still tell the engine — quiet if the route is missing.
        try {
          await vaultGitEnable(false, false);
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          if (!isMissingCapabilityError(message)) {
            this.error = friendlySettingsError(message, "Versions");
            throw err;
          }
          this.unsupported = true;
        }
        return;
      }
      const result = await vaultGitEnable(enabled, initIfNeeded);
      this.status = result.status;
      this.detect = await vaultGitDetect();
      this.unsupported = false;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (isMissingCapabilityError(message)) {
        this.unsupported = true;
        workshopDefaults.draft = {
          ...workshopDefaults.draft,
          vaultGitEnabled: false,
        };
        try {
          await workshopDefaults.save();
        } catch {
          /* best effort */
        }
        this.markDisabledLocally();
        this.unsupported = true;
      }
      this.error = friendlySettingsError(message, "Versions");
      throw err;
    } finally {
      this.busy = false;
    }
  }

  private surfaceError(err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    if (isMissingCapabilityError(message)) this.unsupported = true;
    this.error = friendlySettingsError(message, "Versions");
  }

  async installGit() {
    this.busy = true;
    this.error = null;
    try {
      this.detect = await vaultGitInstall();
      this.status = await vaultGitStatus();
    } catch (err) {
      this.surfaceError(err);
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
      this.surfaceError(err);
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
      this.surfaceError(err);
    } finally {
      this.busy = false;
    }
  }

  async saveVersion(message: string, paths?: string[]) {
    this.busy = true;
    this.error = null;
    try {
      await vaultGitCommit(message, paths);
      await this.refresh({ force: true });
      if (this.panelOpen) {
        await this.loadHistory(paths?.[0]);
      }
    } catch (err) {
      this.surfaceError(err);
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
      await this.refresh({ force: true });
    } catch (err) {
      this.surfaceError(err);
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
      this.surfaceError(err);
    } finally {
      this.busy = false;
    }
  }

  async loadWorktrees() {
    try {
      this.worktrees = await vaultGitWorktrees();
    } catch (err) {
      this.surfaceError(err);
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
