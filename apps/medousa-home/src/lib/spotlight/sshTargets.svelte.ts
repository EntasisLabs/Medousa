import { sshTargets, type SshTarget } from "$lib/utils/sshApi";

/** Owned by an open Spotlight; late responses cannot repopulate a closed or switched view. */
export class SpotlightSshTargets {
  targets = $state<SshTarget[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  private disposed = false;
  constructor(readonly scope: string, private readonly list = sshTargets) {}
  async refresh() {
    if (this.disposed || this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const targets = await this.list();
      if (!this.disposed) this.targets = targets;
    } catch (cause) {
      if (!this.disposed) this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (!this.disposed) this.loading = false;
    }
  }
  dispose() { this.disposed = true; this.targets = []; }
}
