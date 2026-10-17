import { listVaultNotes } from "$lib/daemon";
import type { VaultNoteSummary } from "$lib/types/vault";

/** Bounded note metadata for related links and Map. Context never reads note bodies. */
export class ContextNotesStore {
  notes = $state<VaultNoteSummary[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  private epoch = 0;
  private pending: Promise<void> | null = null;

  refresh(): Promise<void> {
    if (this.pending) return this.pending;
    const pending = this.load(this.epoch).finally(() => { if (this.pending === pending) this.pending = null; });
    this.pending = pending;
    return pending;
  }

  resetForWorkshopSwitch() {
    this.epoch += 1;
    this.pending = null;
    this.notes = [];
    this.loading = false;
    this.error = null;
  }

  private async load(epoch: number) {
    this.loading = true;
    this.error = null;
    try {
      const response = await listVaultNotes({ limit: 200 });
      if (epoch !== this.epoch) return;
      this.notes = response.notes;
    } catch (error) {
      if (epoch !== this.epoch) return;
      this.error = error instanceof Error ? error.message : String(error);
    } finally {
      if (epoch === this.epoch) this.loading = false;
    }
  }
}

export const contextNotes = new ContextNotesStore();
