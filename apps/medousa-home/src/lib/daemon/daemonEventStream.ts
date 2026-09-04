import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  daemonStreamCancel,
  daemonStreamStart,
  type OperationId,
} from "$lib/daemon/contractClient";
import { isTauri } from "$lib/window";

export type DaemonStreamFailure = {
  message: string;
  recoverable?: boolean;
  transport?: string;
  stage?: string;
};

export type DaemonEventConnection = {
  close: () => void;
  readonly closed: boolean;
};

export type OpenDaemonEventStreamOptions<T> = {
  operation: OperationId;
  pathParams?: Record<string, string>;
  query?: Record<string, string>;
  /** Exact paired workshop that owns this stream, when it is not the parent. */
  executionRuntimeId?: string | null;
  /** Browser-only compatibility path. Native Home never puts credentials here. */
  browserUrl: () => Promise<string>;
  browserEvent: string;
  onEvent: (event: T) => void;
  onOpen?: () => void;
  onError: (error: DaemonStreamFailure) => void;
};

let streamSequence = 0;

export function nextDaemonStreamHandle(operation: OperationId): string {
  streamSequence = (streamSequence + 1) % Number.MAX_SAFE_INTEGER;
  const safeOperation = operation.replace(/[^A-Za-z0-9._:-]/g, "-");
  return `${safeOperation}-${Date.now().toString(36)}-${streamSequence.toString(36)}`;
}

async function openNativeDaemonEventStream<T>(
  options: OpenDaemonEventStreamOptions<T>,
): Promise<DaemonEventConnection> {
  const handle = nextDaemonStreamHandle(options.operation);
  const eventName = `daemon-stream://${handle}/event`;
  const errorName = `daemon-stream://${handle}/error`;
  const listeners: UnlistenFn[] = [];
  let closed = false;

  const releaseListeners = () => {
    while (listeners.length > 0) listeners.pop()?.();
  };
  const close = () => {
    if (closed) return;
    closed = true;
    releaseListeners();
    void daemonStreamCancel(handle);
  };

  listeners.push(
    await listen<T>(eventName, ({ payload }) => {
      if (!closed) options.onEvent(payload);
    }),
  );
  listeners.push(
    await listen<DaemonStreamFailure>(errorName, ({ payload }) => {
      if (closed) return;
      close();
      options.onError(payload);
    }),
  );

  try {
    await daemonStreamStart(
      options.operation,
      options.pathParams,
      options.query,
      handle,
      options.executionRuntimeId,
    );
    if (!closed) options.onOpen?.();
    return {
      close,
      get closed() {
        return closed;
      },
    };
  } catch (error) {
    close();
    throw error;
  }
}

async function openBrowserDaemonEventStream<T>(
  options: OpenDaemonEventStreamOptions<T>,
): Promise<DaemonEventConnection> {
  if (typeof EventSource === "undefined") {
    throw new Error("EventSource is unavailable");
  }
  const source = new EventSource(await options.browserUrl());
  let closed = false;
  source.addEventListener(options.browserEvent, (message) => {
    try {
      options.onEvent(JSON.parse((message as MessageEvent<string>).data) as T);
    } catch {
      // Malformed frames are ignored; the next valid cursor advances normally.
    }
  });
  source.onopen = () => options.onOpen?.();
  source.onerror = () => {
    closed = true;
    source.close();
    options.onError({
      message: "Daemon event stream disconnected",
      recoverable: true,
      transport: "browser",
      stage: "read",
    });
  };
  return {
    close: () => {
      closed = true;
      source.close();
    },
    get closed() {
      return closed;
    },
  };
}

/**
 * Open an authenticated daemon SSE operation. Tauri listens before starting
 * the native stream, so event zero cannot race the IPC subscription.
 */
export function openDaemonEventStream<T>(
  options: OpenDaemonEventStreamOptions<T>,
): Promise<DaemonEventConnection> {
  return isTauri()
    ? openNativeDaemonEventStream(options)
    : openBrowserDaemonEventStream(options);
}
