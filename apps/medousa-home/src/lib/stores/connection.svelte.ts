import type { DaemonHealth } from "$lib/daemon";

/** Shared Medousa connection state (desktop + mobile shells). */
class ConnectionStore {
  health = $state<DaemonHealth | null>(null);
  recovering = $state(false);
  private trafficOnline = $state(false);
  trafficRevision = 0;

  /** Real stream data is stronger evidence than an older failed probe. */
  noteTraffic() {
    this.trafficRevision += 1;
    this.trafficOnline = true;
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
    } else if (!health.ok && probeTrafficRevision === this.trafficRevision) {
      this.trafficOnline = false;
    }
  }

  setRecovering(active: boolean) {
    this.recovering = active;
  }
}

export const connection = new ConnectionStore();
