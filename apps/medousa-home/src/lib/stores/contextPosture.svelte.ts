import { listLocusNodes } from "$lib/daemon";
import type { LocusNodeSummary } from "$lib/types/locus";

export class ContextPostureStore {
  nodes = $state<LocusNodeSummary[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  async refresh(limit = 120) {
    this.loading = true;
    this.error = null;
    try {
      const response = await listLocusNodes({ limit });
      this.nodes = response.nodes;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      this.nodes = [];
    } finally {
      this.loading = false;
    }
  }
}

export const contextPosture = new ContextPostureStore();
