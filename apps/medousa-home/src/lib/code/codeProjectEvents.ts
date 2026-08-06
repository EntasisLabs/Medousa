/**
 * Resumable Forge project-event stream for Code buffer reconciliation.
 * Daemon authority: GET /v1/forge/items/{id}/project-events?since=
 */

import {
  forgeProjectEventsUrl,
  type ForgeProjectEvent,
  type ForgeProjectEventKind,
} from "$lib/forge";
import {
  DEFAULT_WORKSPACE_BACKOFF,
  ReconnectScheduler,
} from "$lib/stream/reconnect";

export type { ForgeProjectEvent, ForgeProjectEventKind };

/** LSP FileChangeType: 1 Created, 2 Changed, 3 Deleted. */
export type WatchedFileChange = { uri: string; type: 1 | 2 | 3 };

export type OpenBufferPlan =
  | { action: "reconcile"; path: string }
  | { action: "rename"; oldPath: string; newPath: string }
  | { action: "delete"; path: string }
  | { action: "reconcile_all" }
  | { action: "ignore" };

export function planOpenBufferAction(event: ForgeProjectEvent): OpenBufferPlan {
  switch (event.kind) {
    case "created":
    case "changed":
      return event.path
        ? { action: "reconcile", path: event.path }
        : { action: "ignore" };
    case "renamed":
      return event.old_path && event.path
        ? { action: "rename", oldPath: event.old_path, newPath: event.path }
        : { action: "ignore" };
    case "deleted":
      return event.path ? { action: "delete", path: event.path } : { action: "ignore" };
    case "git_status":
    case "snapshot":
      return { action: "reconcile_all" };
    default:
      return { action: "ignore" };
  }
}

export function watchedFileChangesForProjectEvent(
  event: ForgeProjectEvent,
  toUri: (path: string) => string,
): WatchedFileChange[] {
  switch (event.kind) {
    case "created":
      return event.path ? [{ uri: toUri(event.path), type: 1 }] : [];
    case "changed":
      return event.path ? [{ uri: toUri(event.path), type: 2 }] : [];
    case "deleted":
      return event.path ? [{ uri: toUri(event.path), type: 3 }] : [];
    case "renamed":
      if (!event.path || !event.old_path) return [];
      return [
        { uri: toUri(event.old_path), type: 3 },
        { uri: toUri(event.path), type: 1 },
      ];
    default:
      return [];
  }
}

export function parseProjectEventPayload(raw: string): ForgeProjectEvent | null {
  try {
    const parsed = JSON.parse(raw) as ForgeProjectEvent;
    if (
      typeof parsed?.seq !== "number" ||
      typeof parsed?.work_id !== "string" ||
      typeof parsed?.kind !== "string"
    ) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

export type CodeProjectEventHandlers = {
  onEvent: (event: ForgeProjectEvent) => void;
  onUnavailable?: () => void;
};

/**
 * EventSource client with `?since=` replay and workspace-style reconnect.
 * Uses browser EventSource against the daemon URL (not forge_request JSON).
 */
export class CodeProjectEventStream {
  private source: EventSource | null = null;
  private workId: string | null = null;
  private lastSeq = 0;
  private connecting = false;
  private readonly reconnect = new ReconnectScheduler({
    policy: DEFAULT_WORKSPACE_BACKOFF,
  });
  private readonly handlers: CodeProjectEventHandlers;

  constructor(handlers: CodeProjectEventHandlers) {
    this.handlers = handlers;
  }

  get currentWorkId(): string | null {
    return this.workId;
  }

  get cursor(): number {
    return this.lastSeq;
  }

  setWorkId(workId: string | null) {
    const next = workId?.trim() || null;
    if (this.workId === next) {
      if (next && !this.source && !this.connecting) void this.connect();
      return;
    }
    this.stopSource();
    this.reconnect.cancel();
    this.workId = next;
    this.lastSeq = 0;
    if (next) void this.connect();
  }

  stop() {
    this.stopSource();
    this.reconnect.cancel();
    this.workId = null;
    this.lastSeq = 0;
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
    const id = this.workId;
    if (!id || this.connecting) return;
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
      const url = await forgeProjectEventsUrl(id, this.lastSeq);
      if (this.workId !== id) {
        this.connecting = false;
        return;
      }
      const source = new EventSource(url);
      this.source = source;
      source.addEventListener("project", (message) => {
        const event = parseProjectEventPayload(
          typeof message.data === "string" ? message.data : "",
        );
        if (!event || event.work_id !== id) return;
        if (event.seq <= this.lastSeq) return;
        this.lastSeq = event.seq;
        this.reconnect.noteSuccess();
        this.handlers.onEvent(event);
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
        if (this.workId !== id) return;
        this.reconnect.schedule(() => void this.connect());
      };
    } catch {
      this.connecting = false;
      if (this.workId === id) {
        this.reconnect.schedule(() => void this.connect());
      }
    }
  }
}
