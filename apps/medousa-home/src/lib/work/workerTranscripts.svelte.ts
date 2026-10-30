import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { getWorkspaceCard } from "$lib/daemon";
import type { WorkerProgress, WorkerToolActivity } from "$lib/types/card";

/**
 * Cursor-style per-worker transcript (tools, thinking, output) keyed by work_id.
 * Pushed live by `card_upserted.worker_progress` on the workspace stream; the
 * card-detail fetch is only a cold-start/backfill path.
 */
export interface WorkerTranscript {
  workId: string;
  sessionId?: string | null;
  parentRuntimeId?: string | null;
  taskPrompt?: string | null;
  steerMessages?: { text: string; at: string }[];
  historyLoaded?: boolean;
  retentionNote?: string | null;
  title: string;
  disposition: "bound" | "parallel";
  model?: string | null;
  executionRuntimeId?: string | null;
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

/** Keep evidence already observed even when the daemon's rolling tail advances. */
function mergeToolRuns(previous: WorkerToolActivity[] = [], incoming?: WorkerToolActivity[]): WorkerToolActivity[] {
  if (!incoming) return previous;
  const merged = new Map(previous.map(run => [run.run_id || `${run.round}:${run.name}:${run.started_at}`, run]));
  for (const run of incoming) {
    const key = run.run_id || `${run.round}:${run.name}:${run.started_at}`;
    const old = merged.get(key);
    // A detail request can resolve after a completion frame was observed.
    if (old && old.status !== "running" && run.status === "running") continue;
    merged.set(key, run);
  }
  return [...merged.values()];
}

export class WorkerTranscriptStore {
  transcripts = $state<Map<string, WorkerTranscript>>(new Map());
  private scope = activeWorkshopId();
  private generation = 0;
  private versions = new Map<string, number>();
  private inflight = new Map<string, Promise<WorkerTranscript | null>>();

  /** Merge a workspace card detail into the transcript for its worker. */
  ingestDetail(
    detail: import("$lib/types/card").WorkCardDetail,
    column: string,
  ): WorkerTranscript | null {
    this.ensureScope();
    if (detail.kind !== "turn_worker") return null;
    const workId = detail.work_id?.trim() || detail.card.id;
    if (!workId) return null;

    const existing = this.transcripts.get(workId);
    if (existing?.sessionId && detail.session_id && existing.sessionId !== detail.session_id) return null;
    const full = detail.worker_transcript;
    if (full && (full.work_id !== workId || full.session_id !== detail.session_id)) return null;
    const terminal = TERMINAL_COLUMNS.has(column) && detail.terminal;
    const disposition = this.dispositionFor(detail);
    const next: WorkerTranscript = {
      workId,
      sessionId: detail.session_id ?? existing?.sessionId,
      parentRuntimeId: full?.parent_runtime_id ?? existing?.parentRuntimeId,
      taskPrompt: full?.task_prompt ?? existing?.taskPrompt ?? detail.task_line,
      steerMessages: full?.steer_messages ?? existing?.steerMessages ?? [],
      historyLoaded: !!full || existing?.historyLoaded || false,
      retentionNote: full?.retention_note ?? existing?.retentionNote,
      title:
        detail.user_ack?.trim() ||
        detail.task_line?.trim() ||
        detail.card.title ||
        workId,
      disposition,
      model: detail.model?.trim() || existing?.model || null,
      executionRuntimeId:
        detail.execution_runtime_id?.trim() || existing?.executionRuntimeId || null,
      statusLine:
        (column === "wrapping_up" ? "Result ready · Awaiting summary" : null) ||
        detail.live_status_line?.trim() ||
        existing?.statusLine ||
        detail.card.status_label ||
        null,
      toolRuns: mergeToolRuns(existing?.toolRuns, detail.live_tool_activity),
      thinking: detail.live_thinking ?? existing?.thinking ?? "",
      output: detail.live_output ?? existing?.output ?? "",
      thinkingStartedAt:
        detail.thinking_started_at ?? existing?.thinkingStartedAt ?? null,
      thinkingFinishedAt:
        detail.thinking_finished_at ?? existing?.thinkingFinishedAt ?? null,
      resultText: full ? full.result_text ?? null
        : (existing?.historyLoaded ? existing.resultText : detail.result_excerpt) ?? existing?.resultText ?? null,
      synthesisText: detail.result_excerpt ?? existing?.synthesisText ?? null,
      streaming: !terminal && column === "in_flight",
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
    this.ensureScope();
    const workId = progress.work_id?.trim();
    if (!workId) return null;

    const existing = this.transcripts.get(workId);
    if (existing?.sessionId && progress.session_id && existing.sessionId !== progress.session_id) return null;
    const terminal = TERMINAL_COLUMNS.has(progress.column) && progress.terminal;
    const next: WorkerTranscript = {
      ...existing,
      workId,
      sessionId: progress.session_id ?? existing?.sessionId,
      title: existing?.title || title?.trim() || workId,
      disposition: existing?.disposition ?? "parallel",
      model: progress.model?.trim() || existing?.model || null,
      executionRuntimeId:
        progress.execution_runtime_id?.trim() || existing?.executionRuntimeId || null,
      statusLine: progress.column === "wrapping_up" ? "Result ready · Awaiting summary" : progress.live_status_line?.trim() || existing?.statusLine || null,
      toolRuns: mergeToolRuns(existing?.toolRuns, progress.live_tool_activity),
      thinking: progress.live_thinking ?? existing?.thinking ?? "",
      output: progress.live_output ?? existing?.output ?? "",
      thinkingStartedAt:
        progress.thinking_started_at ?? existing?.thinkingStartedAt ?? null,
      thinkingFinishedAt:
        progress.thinking_finished_at ?? existing?.thinkingFinishedAt ?? null,
      resultText: (existing?.historyLoaded ? existing.resultText : progress.result_excerpt) ?? existing?.resultText ?? null,
      synthesisText: progress.result_excerpt ?? existing?.synthesisText ?? null,
      streaming: !terminal && progress.column === "in_flight",
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
    this.ensureScope();
    return this.transcripts.get(workId) ?? null;
  }

  async refresh(workId: string, force = false): Promise<WorkerTranscript | null> {
    this.ensureScope();
    const trimmed = workId.trim();
    if (!trimmed) return null;
    if (!force) {
      const existing = this.transcripts.get(trimmed);
      if (existing?.terminal && existing.historyLoaded) return existing;
    }
    const inflight = this.inflight.get(trimmed);
    if (inflight) return inflight;
    const scope = this.scope;
    const generation = this.generation;
    const version = this.versions.get(trimmed) ?? 0;
    const current = () => scope === activeWorkshopId() && generation === this.generation && version === (this.versions.get(trimmed) ?? 0);
    const promise = (async () => {
      try {
        const detail = await getWorkspaceCard(trimmed, undefined, scope);
        if (!current() || (detail.work_id?.trim() || detail.card.id) !== trimmed) return null;
        return this.ingestDetail(detail, detail.card.column);
      } catch {
        if (!current()) return null;
        return this.transcripts.get(trimmed) ?? null;
      } finally {
        if (current()) this.inflight.delete(trimmed);
      }
    })();
    this.inflight.set(trimmed, promise);
    return promise;
  }

  clear(workId: string) {
    this.versions.set(workId, (this.versions.get(workId) ?? 0) + 1);
    this.inflight.delete(workId);
    const nextMap = new Map(this.transcripts);
    nextMap.delete(workId);
    this.transcripts = nextMap;
  }

  private ensureScope() {
    const scope = activeWorkshopId();
    if (this.scope === scope) return;
    this.scope = scope;
    this.generation++;
    this.transcripts = new Map();
    this.inflight.clear();
    this.versions.clear();
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
