import { listToolHistorySlices, promoteWorkflowFromSlice } from "$lib/daemon";
import { formatDaemonErrorSummary } from "$lib/utils/formatDaemonError";
import type {
  ToolHistoryRunEntry,
  ToolHistorySliceRef,
  WorkflowFromSliceResponse,
} from "$lib/types/toolHistory";
import { sliceRefFromRun } from "$lib/types/toolHistory";

export class ToolHistoryStore {
  runs = $state<ToolHistoryRunEntry[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  actionMessage = $state<string | null>(null);
  promoting = $state(false);
  selectedIds = $state<Set<string>>(new Set());

  async refresh(options?: {
    limit?: number;
    sessionId?: string;
    toolFilter?: string;
    keyword?: string;
  }) {
    this.loading = true;
    try {
      const response = await listToolHistorySlices(options);
      this.runs = response.runs;
      this.error = null;
    } catch (err) {
      if (this.runs.length === 0) {
        this.error = formatDaemonErrorSummary(err);
      }
    } finally {
      this.loading = false;
    }
  }

  toggleSelected(entryId: string) {
    const next = new Set(this.selectedIds);
    if (next.has(entryId)) {
      next.delete(entryId);
    } else {
      next.add(entryId);
    }
    this.selectedIds = next;
  }

  clearSelection() {
    this.selectedIds = new Set();
  }

  selectedRuns(): ToolHistoryRunEntry[] {
    return this.runs.filter((entry) => this.selectedIds.has(entry.entry_id));
  }

  selectedRefs(): ToolHistorySliceRef[] {
    return this.selectedRuns().map(sliceRefFromRun);
  }

  formatTimestamp(value: string | null | undefined): string {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }

  statusLabel(status: string | null | undefined): string {
    if (!status) return "—";
    if (status === "succeeded") return "Succeeded";
    if (status === "failed") return "Failed";
    if (status === "running") return "Running";
    return status;
  }

  async promoteSelection(name?: string, run = false): Promise<WorkflowFromSliceResponse> {
    const refs = this.selectedRefs();
    if (refs.length === 0) {
      throw new Error("Select at least one tool run.");
    }
    this.promoting = true;
    this.actionMessage = null;
    try {
      const response = await promoteWorkflowFromSlice({
        refs,
        name: name?.trim() || null,
        run,
      });
      this.actionMessage = run
        ? `Flow enqueued · ${response.workflow_id ?? "workflow"}`
        : `Draft ready · ${response.promoted_count} step(s)`;
      this.clearSelection();
      return response;
    } catch (err) {
      this.actionMessage = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.promoting = false;
    }
  }

  async promoteRef(
    ref: ToolHistorySliceRef,
    name?: string,
    run = false,
  ): Promise<WorkflowFromSliceResponse> {
    this.promoting = true;
    this.actionMessage = null;
    try {
      const response = await promoteWorkflowFromSlice({
        refs: [ref],
        name: name?.trim() || null,
        run,
      });
      this.actionMessage = run
        ? `Flow step enqueued · ${response.workflow_id ?? "workflow"}`
        : "Flow step added to draft";
      return response;
    } catch (err) {
      this.actionMessage = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.promoting = false;
    }
  }
}

export const toolHistory = new ToolHistoryStore();
