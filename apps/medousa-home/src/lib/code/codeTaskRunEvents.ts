/**
 * Resumable Forge task-run output stream.
 * Daemon: GET /v1/forge/items/{id}/task-runs/{run_id}/events?since=
 */

import {
  forgeTaskRunEventsUrl,
  type ProjectTaskOutputEvent,
  type ProjectTaskResult,
} from "$lib/forge";
import {
  DEFAULT_WORKSPACE_BACKOFF,
  ReconnectScheduler,
} from "$lib/stream/reconnect";
import {
  openDaemonEventStream,
  type DaemonEventConnection,
} from "$lib/daemon/daemonEventStream";

export type { ProjectTaskOutputEvent };

export function parseTaskRunEventPayload(raw: unknown): ProjectTaskOutputEvent | null {
  try {
    const parsed = (typeof raw === "string"
      ? JSON.parse(raw)
      : raw) as ProjectTaskOutputEvent;
    if (
      typeof parsed?.seq !== "number" ||
      typeof parsed?.run_id !== "string" ||
      typeof parsed?.kind !== "string"
    ) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

export function taskRunEventIsTerminal(event: ProjectTaskOutputEvent): boolean {
  return event.kind === "state" && event.result != null;
}

export type CodeTaskRunEventHandlers = {
  onEvent: (event: ProjectTaskOutputEvent) => void;
  onUnavailable?: () => void;
  onTerminal?: (result: ProjectTaskResult | null, state: string | null) => void;
};

/**
 * EventSource client with `?since=` replay for one task run.
 * Stops after a terminal state event that includes the final result.
 */
export class CodeTaskRunEventStream {
  private source: DaemonEventConnection | null = null;
  private workId: string | null = null;
  private runId: string | null = null;
  /** Last applied sequence; -1 means no event has been applied yet. */
  private lastSeq = -1;
  private connecting = false;
  private closed = false;
  private readonly reconnect = new ReconnectScheduler({
    policy: DEFAULT_WORKSPACE_BACKOFF,
  });
  private readonly handlers: CodeTaskRunEventHandlers;

  constructor(handlers: CodeTaskRunEventHandlers) {
    this.handlers = handlers;
  }

  get cursor(): number {
    return this.lastSeq + 1;
  }

  start(workId: string, runId: string, since = 0) {
    this.stopSource();
    this.reconnect.cancel();
    this.closed = false;
    this.workId = workId.trim() || null;
    this.runId = runId.trim() || null;
    this.lastSeq = Math.max(-1, since - 1);
    if (this.workId && this.runId) void this.connect();
  }

  stop() {
    this.closed = true;
    this.stopSource();
    this.reconnect.cancel();
    this.workId = null;
    this.runId = null;
    this.lastSeq = -1;
  }

  teardown() {
    this.stop();
    this.reconnect.teardown();
  }

  private stopSource() {
    this.connecting = false;
    if (this.source) {
      this.source.close();
      this.source = null;
    }
  }

  private async connect() {
    const workId = this.workId;
    const runId = this.runId;
    if (!workId || !runId || this.connecting || this.closed) return;
    this.connecting = true;
    if (this.source) {
      this.source.close();
      this.source = null;
    }
    let source: DaemonEventConnection | null = null;
    try {
      source = await openDaemonEventStream<ProjectTaskOutputEvent>({
        operation: "forge.items.by_work_id.task_runs.by_run_id.events.get",
        pathParams: { work_id: workId, run_id: runId },
        query: { since: String(this.lastSeq + 1) },
        browserUrl: () => forgeTaskRunEventsUrl(workId, runId, this.lastSeq + 1),
        browserEvent: "task",
        onEvent: (payload) => {
          const event = parseTaskRunEventPayload(payload);
          if (!event || event.run_id !== runId) return;
          if (event.seq <= this.lastSeq) return;
          this.lastSeq = event.seq;
          this.reconnect.noteSuccess();
          this.handlers.onEvent(event);
          if (taskRunEventIsTerminal(event)) {
            this.handlers.onTerminal?.(event.result ?? null, event.state ?? null);
            this.stop();
          }
        },
        onOpen: () => {
          this.connecting = false;
          this.reconnect.noteSuccess();
        },
        onError: () => {
          this.connecting = false;
          if (source && this.source === source) this.source = null;
          if (this.closed || this.workId !== workId || this.runId !== runId) return;
          this.reconnect.schedule(() => void this.connect());
        },
      });
      if (this.workId !== workId || this.runId !== runId || this.closed) {
        source.close();
        this.connecting = false;
        return;
      }
      if (source.closed) return;
      this.source = source;
    } catch {
      source?.close();
      this.connecting = false;
      this.handlers.onUnavailable?.();
      if (!this.closed && this.workId === workId && this.runId === runId) {
        this.reconnect.schedule(() => void this.connect());
      }
    }
  }
}
