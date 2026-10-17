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

  private refreshEpoch = 0;
  private refreshKey = "";
  private refreshPending: Promise<void> | null = null;

  refresh(options?: { sessionId?: string; q?: string; limit?: number }): Promise<void> {
    const key = JSON.stringify(options ?? {});
    if (this.refreshPending && this.refreshKey === key) return this.refreshPending;
    const epoch = ++this.refreshEpoch;
    this.refreshKey = key;
    const pending = this.load(options, epoch).finally(() => {
      if (this.refreshPending === pending) this.refreshPending = null;
    });
    this.refreshPending = pending;
    return pending;
  }

  private async load(options: { sessionId?: string; q?: string; limit?: number } | undefined, epoch: number) {
    this.loading = true;
    this.error = null;
    try {
      const response = await listLocusNodes({ sessionId: options?.sessionId, q: options?.q, limit: options?.limit ?? 200 });
      if (epoch === this.refreshEpoch) this.nodes = response.nodes;
    } catch (err) {
      if (epoch === this.refreshEpoch) this.error = err instanceof Error ? err.message : String(err);
    } finally {
      if (epoch === this.refreshEpoch) this.loading = false;
    }
  }

  resetForWorkshopSwitch() {
    this.refreshEpoch += 1;
    this.refreshPending = null;
    this.nodes = [];
    this.loading = false;
    this.error = null;
    this.railFocusSyncKey = null;
    this.clearDetail();
  }

  private detailEpoch = 0;
  private detailKey: string | null = null;

  async loadDetail(syncKey: string) {
    if (this.detailKey === syncKey && (this.detailLoading || this.detail?.node.sync_key === syncKey)) return;
    const epoch = ++this.detailEpoch;
    this.detailKey = syncKey;
    this.detail = null;
    this.detailLoading = true;
    this.detailError = null;
    try {
      const detail = await getLocusNode(syncKey);
      if (epoch !== this.detailEpoch) return;
      this.detail = detail;
    } catch (err) {
      if (epoch !== this.detailEpoch) return;
      this.detailError = err instanceof Error ? err.message : String(err);
      this.detail = null;
    } finally {
      if (epoch === this.detailEpoch) this.detailLoading = false;
    }
  }

  clearDetail() {
    this.detailEpoch += 1;
    this.detailKey = null;
    this.detailLoading = false;
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
