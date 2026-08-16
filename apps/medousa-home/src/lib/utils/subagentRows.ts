import { chat } from "$lib/stores/chat.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { workerTranscripts } from "$lib/work/workerTranscripts.svelte";
import type { WorkerTranscript } from "$lib/work/workerTranscripts.svelte";
import type { ToolRunState } from "$lib/types/chat";
import type { WorkCardDetail, WorkerToolActivity } from "$lib/types/card";

/**
 * One inline sub-agent beat in the host thread: the footnote header plus the
 * evidence needed to peek (tools with their arguments, reasoning tail).
 */
export interface SubagentRow {
  workId: string;
  title: string;
  disposition: "bound" | "parallel";
  model?: string | null;
  statusLine: string;
  toolRuns: ToolRunState[];
  thinking: string;
  /** Wall time between first and last reasoning chunk, for "Thought for Ns". */
  thinkingSeconds: number | null;
  streaming: boolean;
  terminal: boolean;
}

/** Worker tool runs share the host's `ToolRunState` shape so evidence renders once. */
export function toolRunsFromWorkerActivity(activity: WorkerToolActivity[]): ToolRunState[] {
  return activity.map((run) => ({
    runId: run.run_id,
    toolName: run.name,
    status:
      run.status === "failed" ? "failed" : run.status === "running" ? "running" : "succeeded",
    round: run.round,
    inputSummary: run.input_summary ?? null,
    inputParams: run.input_params ?? undefined,
    outputSummary: run.output_summary ?? null,
  }));
}

function thinkingSecondsFor(transcript: WorkerTranscript | null): number | null {
  const start = transcript?.thinkingStartedAt;
  const end = transcript?.thinkingFinishedAt;
  if (!start || !end) return null;
  const seconds = (Date.parse(end) - Date.parse(start)) / 1000;
  return Number.isFinite(seconds) && seconds >= 0 ? seconds : null;
}

function titleForDetail(detail: WorkCardDetail): string {
  const ack = detail.user_ack?.trim();
  if (ack) return ack;
  const task = detail.task_line?.trim();
  if (task) return task;
  return detail.card.title?.trim() || detail.work_id?.trim() || "Subagent";
}

function dispositionForDetail(detail: WorkCardDetail): "bound" | "parallel" {
  const subtitle = detail.subtitle?.toLowerCase() ?? "";
  if (subtitle.includes("bound") || subtitle.includes("workshop")) return "bound";
  return "parallel";
}

function rowFromTranscript(
  workId: string,
  transcript: WorkerTranscript | null,
  base: Pick<SubagentRow, "title" | "disposition" | "model" | "statusLine" | "terminal">,
): SubagentRow {
  return {
    workId,
    ...base,
    toolRuns: toolRunsFromWorkerActivity(transcript?.toolRuns ?? []),
    thinking: transcript?.thinking ?? "",
    thinkingSeconds: thinkingSecondsFor(transcript),
    streaming: !base.terminal,
  };
}

/**
 * Sub-agent rows for a host session, in workspace-card order. Callers that need
 * chronological placement should anchor by `workId` via {@link subagentRowMap}.
 */
export function subagentRowsForSession(sessionId: string): SubagentRow[] {
  const rows: SubagentRow[] = [];
  const seen = new Set<string>();

  for (const card of workspace.cards) {
    const detail = workspace.cardDetailsCache.get(card.id);
    if (!detail || detail.kind !== "turn_worker") continue;
    if (detail.session_id?.trim() !== sessionId) continue;
    const workId = detail.work_id?.trim() || card.id;
    const transcript = workerTranscripts.transcriptFor(workId);
    const terminal =
      card.column === "done" || (card.column === "blocked" && detail.terminal);
    seen.add(workId);
    rows.push(
      rowFromTranscript(workId, transcript, {
        title: titleForDetail(detail),
        disposition: dispositionForDetail(detail),
        model: detail.model?.trim() || transcript?.model || null,
        statusLine:
          transcript?.statusLine?.trim() ||
          detail.live_status_line?.trim() ||
          card.status_label ||
          "Working…",
        terminal,
      }),
    );
  }

  // Workers linked in chat that haven't projected a card yet still get a beat,
  // otherwise the thread goes silent between delegation and the first card.
  for (const [workId, link] of chat.workers) {
    if (seen.has(workId)) continue;
    if (link.sessionId !== sessionId) continue;
    const transcript = workerTranscripts.transcriptFor(workId);
    rows.push(
      rowFromTranscript(workId, transcript, {
        title: transcript?.title ?? "Subagent",
        disposition: transcript?.disposition ?? "parallel",
        model: transcript?.model ?? null,
        statusLine: transcript?.statusLine ?? "Working…",
        terminal: transcript?.terminal ?? false,
      }),
    );
  }

  return rows;
}

/** Rows keyed by `work_id` so the message list can anchor each beat in place. */
export function subagentRowMap(sessionId: string): Map<string, SubagentRow> {
  return new Map(subagentRowsForSession(sessionId).map((row) => [row.workId, row]));
}
