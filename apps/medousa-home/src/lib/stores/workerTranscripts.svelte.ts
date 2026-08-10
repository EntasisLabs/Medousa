import { getWorkspaceCard } from "$lib/daemon";
import type { WorkerProgress, WorkerToolActivity } from "$lib/types/card";

/**
 * Cursor-style per-worker transcript (tools, thinking, output) keyed by work_id.
 * Pushed live by `card_upserted.worker_progress` on the workspace stream; the
 * card-detail fetch is only a cold-start/backfill path.
 */
export interface WorkerTranscript {
  workId: string;
  title: string;
  disposition: "bound" | "parallel";
  model?: string | null;
  statusLine?: string | null;
  toolRuns: WorkerToolActivity[];
  thinking: string;
  output: string;
  thinkingStartedAt?: string | null;
  thinkingFinishedAt?: string | null;
  resultText?: string | null;
  synthesisText?: string | null;
  streaming: boolean;
  terminal: boolean;
  error?: string | null;
  updatedAt?: string | null;
}

const TERMINAL_COLUMNS = new Set(["done", "blocked"]);

export class WorkerTranscriptStore {
  transcripts = $state<Map<string, WorkerTranscript>>(new Map());
  private inflight = new Map<string, Promise<WorkerTranscript | null>>();

  /** Merge a workspace card detail into the transcript for its worker. */
  ingestDetail(
    detail: import("$lib/types/card").WorkCardDetail,
    column: string,
  ): WorkerTranscript | null {
    if (detail.kind !== "turn_worker") return null;
    const workId = detail.work_id?.trim() || detail.card.id;
    if (!workId) return null;

    const existing = this.transcripts.get(workId);
    const terminal = TERMINAL_COLUMNS.has(column) && detail.terminal;
    const disposition = this.dispositionFor(detail);
    const next: WorkerTranscript = {
      workId,
      title:
        detail.user_ack?.trim() ||
        detail.task_line?.trim() ||
        detail.card.title ||
        workId,
      disposition,
      model: detail.model?.trim() || existing?.model || null,
      statusLine:
        detail.live_status_line?.trim() ||
        existing?.statusLine ||
        detail.card.status_label ||
        null,
      toolRuns: detail.live_tool_activity ?? existing?.toolRuns ?? [],
      thinking: detail.live_thinking ?? existing?.thinking ?? "",
      output: detail.live_output ?? existing?.output ?? "",
      thinkingStartedAt:
        detail.thinking_started_at ?? existing?.thinkingStartedAt ?? null,
      thinkingFinishedAt:
        detail.thinking_finished_at ?? existing?.thinkingFinishedAt ?? null,
      resultText: detail.result_excerpt ?? existing?.resultText ?? null,
      synthesisText: detail.result_excerpt ?? existing?.synthesisText ?? null,
      streaming: !terminal,
      terminal,
      error: detail.error ?? existing?.error ?? null,
      updatedAt: detail.card.updated_at_utc ?? existing?.updatedAt ?? null,
    };
    const nextMap = new Map(this.transcripts);
    nextMap.set(workId, next);
    this.transcripts = nextMap;
    return next;
  }

  /**
   * Merge a live progress frame from the workspace stream. Unlike `ingestDetail`
   * this can arrive before we ever fetched the card, so it seeds a transcript
   * from the card title we already have on the board.
   */
  ingestProgress(progress: WorkerProgress, title?: string): WorkerTranscript | null {
    const workId = progress.work_id?.trim();
    if (!workId) return null;

    const existing = this.transcripts.get(workId);
    const terminal = TERMINAL_COLUMNS.has(progress.column) && progress.terminal;
    const next: WorkerTranscript = {
      workId,
      title: existing?.title || title?.trim() || workId,
      disposition: existing?.disposition ?? "parallel",
      model: progress.model?.trim() || existing?.model || null,
      statusLine: progress.live_status_line?.trim() || existing?.statusLine || null,
      toolRuns: progress.live_tool_activity ?? existing?.toolRuns ?? [],
      thinking: progress.live_thinking ?? existing?.thinking ?? "",
      output: progress.live_output ?? existing?.output ?? "",
      thinkingStartedAt:
        progress.thinking_started_at ?? existing?.thinkingStartedAt ?? null,
      thinkingFinishedAt:
        progress.thinking_finished_at ?? existing?.thinkingFinishedAt ?? null,
      resultText: progress.result_excerpt ?? existing?.resultText ?? null,
      synthesisText: progress.result_excerpt ?? existing?.synthesisText ?? null,
      streaming: !terminal,
      terminal,
      error: existing?.error ?? null,
      updatedAt: new Date().toISOString(),
    };
    const nextMap = new Map(this.transcripts);
    nextMap.set(workId, next);
    this.transcripts = nextMap;
    return next;
  }

  transcriptFor(workId: string): WorkerTranscript | null {
    return this.transcripts.get(workId) ?? null;
  }

  async refresh(workId: string, force = false): Promise<WorkerTranscript | null> {
    const trimmed = workId.trim();
    if (!trimmed) return null;
    if (!force) {
      const existing = this.transcripts.get(trimmed);
      if (existing?.terminal) return existing;
    }
    const inflight = this.inflight.get(trimmed);
    if (inflight) return inflight;
    const promise = (async () => {
      try {
        const detail = await getWorkspaceCard(trimmed);
        return this.ingestDetail(detail, detail.card.column);
      } catch {
        return this.transcripts.get(trimmed) ?? null;
      } finally {
        this.inflight.delete(trimmed);
      }
    })();
    this.inflight.set(trimmed, promise);
    return promise;
  }

  clear(workId: string) {
    const nextMap = new Map(this.transcripts);
    nextMap.delete(workId);
    this.transcripts = nextMap;
  }

  private dispositionFor(
    detail: import("$lib/types/card").WorkCardDetail,
  ): "bound" | "parallel" {
    const subtitle = detail.subtitle?.toLowerCase() ?? "";
    if (subtitle.includes("bound") || subtitle.includes("workshop")) return "bound";
    return "parallel";
  }
}

export const workerTranscripts = new WorkerTranscriptStore();
