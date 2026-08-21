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

export type { ProjectTaskOutputEvent };

export function parseTaskRunEventPayload(raw: string): ProjectTaskOutputEvent | null {
  try {
    const parsed = JSON.parse(raw) as ProjectTaskOutputEvent;
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
  private source: EventSource | null = null;
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
    if (typeof EventSource === "undefined") {
      this.handlers.onUnavailable?.();
      return;
    }
    this.connecting = true;
    if (this.source) {
      this.source.close();
      this.source = null;
    }
    try {
      const url = await forgeTaskRunEventsUrl(workId, runId, this.lastSeq + 1);
      if (this.workId !== workId || this.runId !== runId || this.closed) {
        this.connecting = false;
        return;
      }
      const source = new EventSource(url);
      this.source = source;
      source.addEventListener("task", (message) => {
        const event = parseTaskRunEventPayload(
          typeof message.data === "string" ? message.data : "",
        );
        if (!event || event.run_id !== runId) return;
        if (event.seq <= this.lastSeq) return;
        this.lastSeq = event.seq;
        this.reconnect.noteSuccess();
        this.handlers.onEvent(event);
        if (taskRunEventIsTerminal(event)) {
          this.handlers.onTerminal?.(event.result ?? null, event.state ?? null);
          this.stop();
        }
      });
      source.onopen = () => {
        this.connecting = false;
        this.reconnect.noteSuccess();
      };
      source.onerror = () => {
        this.connecting = false;
        if (this.source === source) {
          source.close();
          this.source = null;
        }
        if (this.closed || this.workId !== workId || this.runId !== runId) return;
        // Server closes the stream after the terminal event; treat clean close as done.
        this.reconnect.schedule(() => void this.connect());
      };
    } catch {
      this.connecting = false;
      if (!this.closed && this.workId === workId && this.runId === runId) {
        this.reconnect.schedule(() => void this.connect());
      }
    }
  }
}
