import {
  fetchAppUpdateStatus,
  type AppUpdateStatus,
} from "$lib/utils/appUpdate";
import { isTauri } from "$lib/window";

class AppUpdateStore {
  status = $state<AppUpdateStatus | null>(null);
  checking = $state(false);
  lastError = $state<string | null>(null);
  /** Quiet boot probe already ran this session. */
  bootChecked = $state(false);

  readonly updateAvailable = $derived(Boolean(this.status?.updateAvailable));

  async check(options?: { quiet?: boolean }): Promise<AppUpdateStatus | null> {
    if (!isTauri()) return null;
    this.checking = true;
    if (!options?.quiet) this.lastError = null;
    try {
      const status = await fetchAppUpdateStatus();
      this.status = status;
      if (status?.error && !options?.quiet) {
        this.lastError = status.error;
      }
      return status;
    } catch (err) {
      const message = err instanceof Error ? err.message : "Could not check for updates.";
      if (!options?.quiet) this.lastError = message;
      return null;
    } finally {
      this.checking = false;
    }
  }

  /** Once per session — used by WorkshopShell boot probe. */
  async bootProbe(): Promise<AppUpdateStatus | null> {
    if (!isTauri() || this.bootChecked) return this.status;
    this.bootChecked = true;
    return this.check({ quiet: true });
  }
}

export const appUpdate = new AppUpdateStore();
