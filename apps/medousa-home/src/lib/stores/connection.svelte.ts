import type { DaemonHealth } from "$lib/daemon";

/** Shared Medousa connection state (desktop + mobile shells). */
class ConnectionStore {
  health = $state<DaemonHealth | null>(null);
  recovering = $state(false);

  get checking(): boolean {
    return this.health === null;
  }

  get online(): boolean {
    return this.health?.ok === true;
  }

  get offline(): boolean {
    return this.health !== null && !this.health.ok;
  }

  setHealth(health: DaemonHealth | null) {
    this.health = health;
  }

  setRecovering(active: boolean) {
    this.recovering = active;
  }
}

export const connection = new ConnectionStore();
