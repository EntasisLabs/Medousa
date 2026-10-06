import type { DaemonHealth } from "$lib/daemon";

/** Shared Medousa connection state (desktop + mobile shells). */
class ConnectionStore {
  health = $state<DaemonHealth | null>(null);
  recovering = $state(false);
  private trafficOnline = $state(false);
  private trafficExpiry: ReturnType<typeof setTimeout> | null = null;
  trafficRevision = 0;

  /** Real stream data is stronger evidence than an older failed probe. */
  noteTraffic() {
    this.trafficRevision += 1;
    this.trafficOnline = true;
    if (this.trafficExpiry !== null) clearTimeout(this.trafficExpiry);
    // Stream heartbeats normally arrive every 30 seconds. A single failed
    // probe must not override fresh authenticated traffic on a working pipe.
    this.trafficExpiry = setTimeout(() => {
      this.trafficOnline = false;
      this.trafficExpiry = null;
    }, 75_000);
  }

  get checking(): boolean {
    return this.health === null && !this.trafficOnline;
  }

  get online(): boolean {
    return this.trafficOnline || this.health?.ok === true;
  }

  get offline(): boolean {
    return !this.trafficOnline && this.health !== null && !this.health.ok;
  }

  setHealth(health: DaemonHealth | null, probeTrafficRevision = this.trafficRevision) {
    this.health = health;
    if (health === null) {
      this.trafficRevision += 1;
      this.trafficOnline = false;
      if (this.trafficExpiry !== null) clearTimeout(this.trafficExpiry);
      this.trafficExpiry = null;
    } else if (!health.ok && probeTrafficRevision === this.trafficRevision && this.trafficExpiry === null) {
      this.trafficOnline = false;
    }
  }

  setRecovering(active: boolean) {
    this.recovering = active;
  }
}

export const connection = new ConnectionStore();
