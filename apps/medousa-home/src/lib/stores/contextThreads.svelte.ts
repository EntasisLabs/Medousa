import { getLocusNode, listLocusNodes } from "$lib/daemon";
import type {
  LocusNodeDetailResponse,
  LocusNodeSummary,
} from "$lib/types/locus";

export class ContextThreadsStore {
  nodes = $state<LocusNodeSummary[]>([]);
  detail = $state<LocusNodeDetailResponse | null>(null);
  loading = $state(false);
  detailLoading = $state(false);
  error = $state<string | null>(null);
  detailError = $state<string | null>(null);
  /** Set by the master rail — ContextPanel consumes and focuses Threads. */
  railFocusSyncKey = $state<string | null>(null);

  async refresh(options?: { sessionId?: string; q?: string; limit?: number }) {
    this.loading = true;
    this.error = null;
    try {
      const response = await listLocusNodes({
        sessionId: options?.sessionId,
        q: options?.q,
        limit: options?.limit ?? 80,
      });
      this.nodes = response.nodes;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      this.nodes = [];
    } finally {
      this.loading = false;
    }
  }

  async loadDetail(syncKey: string) {
    this.detailLoading = true;
    this.detailError = null;
    try {
      this.detail = await getLocusNode(syncKey);
    } catch (err) {
      this.detailError = err instanceof Error ? err.message : String(err);
      this.detail = null;
    } finally {
      this.detailLoading = false;
    }
  }

  clearDetail() {
    this.detail = null;
    this.detailError = null;
  }

  focusThreadFromRail(syncKey: string) {
    const trimmed = syncKey.trim();
    if (!trimmed) return;
    this.railFocusSyncKey = trimmed;
    void this.loadDetail(trimmed);
  }

  consumeRailFocus(): string | null {
    const key = this.railFocusSyncKey;
    this.railFocusSyncKey = null;
    return key;
  }
}

export const contextThreads = new ContextThreadsStore();
