/**
 * Discover + connect to the workshop LSP Interoperability Orchestrator.
 * Prefer daemon-proxied `/v1/code/lsp` so remote Connection works.
 * Falls back to in-daemon Grapheme LSP when the coding engine is unavailable.
 */

import type { Transport } from "@codemirror/lsp-client";
import { invoke } from "@tauri-apps/api/core";
import {
  LSPClient,
  languageServerExtensions,
} from "@codemirror/lsp-client";
import {
  MedousaCodeWorkspace,
  MedousaCodeWorkspaceBridge,
} from "$lib/code/medousaCodeWorkspace";
import {
  canonicalCodeDocumentUri,
  pathToFileUri,
  validatedCodeLanguageRootUri,
} from "$lib/code/codeDocumentUri";

export { pathToFileUri } from "$lib/code/codeDocumentUri";
import {
  daemonWebSocketUrl,
  getCodingEngineInfo,
  getDaemonUrl,
  getGraphemeLspWorkspace,
} from "$lib/daemon";
import type { GraphemeLspWorkspaceResponse } from "$lib/types/grapheme";

type CloseableTransport = {
  transport: Transport;
  close: () => void;
  closed: Promise<CodeLspSocketClose>;
};

export type CodeLspSocketClose = {
  expected: boolean;
  code: number;
  reason: string;
  clean: boolean;
};

export type CodeLanguageServerEvent =
  | {
      kind: "progress";
      token: string;
      progressKind: "begin" | "report" | "end";
      title: string;
      message: string;
      percentage: number | null;
    }
  | {
      kind: "log";
      level: "error" | "warning" | "info" | "log";
      message: string;
    };

export function codeLanguageServerEventFromMessage(
  raw: string,
): CodeLanguageServerEvent | null {
  let value: Record<string, unknown>;
  try {
    value = JSON.parse(raw) as Record<string, unknown>;
  } catch {
    return null;
  }
  const params = value.params as Record<string, unknown> | undefined;
  if (value.method === "$/progress" && params) {
    const progress = params.value as Record<string, unknown> | undefined;
    if (!progress) return null;
    const progressKind = progress.kind;
    if (progressKind !== "begin" && progressKind !== "report" && progressKind !== "end") {
      return null;
    }
    const percentage =
      typeof progress.percentage === "number"
        ? Math.max(0, Math.min(100, progress.percentage))
        : null;
    return {
      kind: "progress",
      token:
        typeof params.token === "string"
          ? params.token
          : JSON.stringify(params.token ?? null),
      progressKind,
      title: typeof progress.title === "string" ? progress.title : "",
      message: typeof progress.message === "string" ? progress.message : "",
      percentage,
    };
  }
  if (
    (value.method === "window/logMessage" || value.method === "window/showMessage") &&
    params &&
    typeof params.message === "string"
  ) {
    const level =
      params.type === 1
        ? "error"
        : params.type === 2
          ? "warning"
          : params.type === 3
            ? "info"
            : "log";
    return { kind: "log", level, message: params.message };
  }
  return null;
}

function createCloseableWebSocketTransport(
  uri: string,
  onServerEvent?: (event: CodeLanguageServerEvent) => void,
): Promise<CloseableTransport> {
  const handlers: Array<(value: string) => void> = [];
  const socket = new WebSocket(uri);
  let expectedClose = false;
  let opened = false;
  let connectionSettled = false;
  let closeSettled = false;
  let resolveClosed!: (value: CodeLspSocketClose) => void;
  const closed = new Promise<CodeLspSocketClose>((resolve) => {
    resolveClosed = resolve;
  });
  const settleClosed = (value: CodeLspSocketClose) => {
    if (closeSettled) return;
    closeSettled = true;
    resolveClosed(value);
  };
  socket.onmessage = (event) => {
    const payload =
      typeof event.data === "string" ? event.data : event.data.toString();
    const serverEvent = codeLanguageServerEventFromMessage(payload);
    if (serverEvent) onServerEvent?.(serverEvent);
    for (const handler of handlers) {
      handler(payload);
    }
  };
  return new Promise((resolve, reject) => {
    socket.onopen = () => {
      opened = true;
      connectionSettled = true;
      const transport: Transport = {
        send(message: string) {
          if (socket.readyState !== WebSocket.OPEN) {
            throw new Error("LSP websocket is closed");
          }
          socket.send(message);
        },
        subscribe(handler: (value: string) => void) {
          handlers.push(handler);
        },
        unsubscribe(handler: (value: string) => void) {
          const index = handlers.indexOf(handler);
          if (index >= 0) handlers.splice(index, 1);
        },
      };
      resolve({
        transport,
        closed,
        close() {
          expectedClose = true;
          handlers.length = 0;
          socket.onmessage = null;
          if (socket.readyState === WebSocket.CONNECTING || socket.readyState === WebSocket.OPEN) {
            socket.close(1000, "workspace released");
          } else {
            settleClosed({ expected: true, code: 1000, reason: "workspace released", clean: true });
          }
        },
      });
    };
    socket.onerror = () => {
      if (!opened && !connectionSettled) {
        connectionSettled = true;
        reject(new Error("LSP websocket failed"));
      } else if (opened) {
        settleClosed({
          expected: expectedClose,
          code: 1006,
          reason: "LSP websocket failed",
          clean: false,
        });
      }
    };
    socket.onclose = (event) => {
      handlers.length = 0;
      settleClosed({
        expected: expectedClose,
        code: event.code,
        reason: event.reason,
        clean: event.wasClean,
      });
      if (!opened && !connectionSettled) {
        connectionSettled = true;
        reject(new Error(event.reason || "LSP websocket closed before connecting"));
      }
    };
  });
}

export async function createWebSocketTransport(uri: string): Promise<Transport> {
  return (await createCloseableWebSocketTransport(uri)).transport;
}

export type CodingEngineInfo = {
  available: boolean;
  url: string;
  health_url: string;
  lsp_url: string;
  daemon_lsp_path: string;
  workspace_root: string;
  workspace_root_uri: string;
  bind: string;
  message: string;
};

export type ConnectOrchestratorLspOptions = {
  language?: string;
  workId?: string;
  documentUri?: string;
  workspaceRoot?: string;
  languageRootUri?: string;
  workspaceBridge?: MedousaCodeWorkspaceBridge;
  onServerEvent?: (event: CodeLanguageServerEvent) => void;
};

export async function connectOrchestratorLspClient(options?: ConnectOrchestratorLspOptions): Promise<{
  client: LSPClient;
  workspace: GraphemeLspWorkspaceResponse;
  via: "orchestrator" | "grapheme";
  close: () => void;
  closed: Promise<CodeLspSocketClose>;
  ready: Promise<null>;
}> {
  const language = (options?.language ?? "grapheme").trim() || "grapheme";
  const graphemeWorkspace = await getGraphemeLspWorkspace();

  try {
    const info = await getCodingEngineInfo();
    if (info.available) {
      const query = new URLSearchParams({ language });
      if (options?.workId) query.set("work_id", options.workId);
      if (options?.documentUri) query.set("document_uri", options.documentUri);
      const path = `${info.daemon_lsp_path || "/v1/code/lsp"}?${query}`;
      const wsUrl = await daemonWebSocketUrl(path);
      const connection = await createCloseableWebSocketTransport(
        wsUrl,
        options?.onServerEvent,
      );
      const rootUri = options?.languageRootUri
        ? canonicalCodeDocumentUri(options.languageRootUri)
        : options?.workspaceRoot
          ? pathToFileUri(options.workspaceRoot)
          : info.workspace_root_uri || graphemeWorkspace.root_uri;
      const client = new LSPClient({
        rootUri,
        timeout: 30_000,
        notificationHandlers: quietShowMessageHandlers(options?.onServerEvent),
        extensions: [
          ...languageServerExtensions(),
          {
            clientCapabilities: {
              workspace: {
                configuration: true,
                workspaceFolders: true,
              },
              window: { workDoneProgress: true },
            },
          },
        ],
        workspace: options?.workspaceBridge
          ? (client) => new MedousaCodeWorkspace(client, options.workspaceBridge!)
          : undefined,
      }).connect(connection.transport);
      const ready = client.initializing;
      void ready.catch(() => {
        client.disconnect();
        connection.close();
      });
      return {
        client,
        workspace: {
          ...graphemeWorkspace,
          root_uri: rootUri,
          root_path: options?.workspaceRoot || info.workspace_root || graphemeWorkspace.root_path,
        },
        via: "orchestrator",
        close: connection.close,
        closed: connection.closed,
        ready,
      };
    }
  } catch {
    if (language !== "grapheme") {
      throw new Error(`Smart editing is unavailable for ${language}`);
    }
    // Grapheme has an in-daemon fallback.
  }

  const wsUrl = await daemonWebSocketUrl("/v1/grapheme/lsp");
  const connection = await createCloseableWebSocketTransport(
    wsUrl,
    options?.onServerEvent,
  );
  const client = new LSPClient({
    rootUri: graphemeWorkspace.root_uri,
    timeout: 30_000,
    notificationHandlers: quietShowMessageHandlers(options?.onServerEvent),
    extensions: languageServerExtensions(),
    workspace: options?.workspaceBridge
      ? (client) => new MedousaCodeWorkspace(client, options.workspaceBridge!)
      : undefined,
  }).connect(connection.transport);
  const ready = client.initializing;
  void ready.catch(() => {
    client.disconnect();
    connection.close();
  });
  return {
    client,
    workspace: graphemeWorkspace,
    via: "grapheme",
    close: connection.close,
    closed: connection.closed,
    ready,
  };
}

export type CodeWorkspaceLspProgress = {
  token: string;
  title: string;
  message: string;
  percentage: number | null;
};

export type CodeWorkspaceLspStatus = {
  phase: "connecting" | "ready" | "reconnecting" | "failed" | "stopped";
  detail: string;
  progress: CodeWorkspaceLspProgress | null;
  /** Server window/showMessage (warnings/errors only — info stays quiet). */
  notice: string | null;
};

type WorkspaceClientEntry = {
  connection: ReturnType<typeof connectOrchestratorLspClient>;
  client: Promise<LSPClient>;
  workspaceBridge: MedousaCodeWorkspaceBridge;
  references: number;
  releaseTimer: ReturnType<typeof setTimeout> | null;
  status: CodeWorkspaceLspStatus;
  listeners: Set<(status: CodeWorkspaceLspStatus) => void>;
  expectedClose: boolean;
  disconnected: boolean;
};

export type CodeWorkspaceLspLease = {
  client: Promise<LSPClient>;
  workspaceBridge: MedousaCodeWorkspaceBridge;
  subscribeStatus: (listener: (status: CodeWorkspaceLspStatus) => void) => () => void;
  restart: () => void;
  release: () => void;
};

const WORKSPACE_CLIENT_RELEASE_DELAY_MS = 1_000;
export const CODE_LSP_MAX_RECONNECT_ATTEMPTS = 3;
const workspaceClients = new Map<string, WorkspaceClientEntry>();

/** Errors that will not recover by reconnecting (path/policy/install). */
export function isPermanentLanguageServiceError(detail: string): boolean {
  const lower = detail.toLowerCase();
  return (
    lower.includes("outside the governed") ||
    lower.includes("outside the coding engine allowlist") ||
    lower.includes("is not installed on this workshop") ||
    lower.includes("was not found on this workshop path") ||
    lower.includes("unknown or unprepared undertaking") ||
    lower.includes("document uri must use the file scheme") ||
    lower.includes("encoded path separator") ||
    lower.includes("document uri is not a valid workshop file path") ||
    lower.includes("document uri contains an encoded path separator")
  );
}

export function codeLspReconnectDelay(attempt: number): number | null {
  if (!Number.isInteger(attempt) || attempt < 1 || attempt > CODE_LSP_MAX_RECONNECT_ATTEMPTS) {
    return null;
  }
  return [250, 750, 1_500][attempt - 1] ?? null;
}

export function codeWorkspaceLspPoolKey(
  workId: string,
  language: string,
  languageRootUri: string,
): string {
  return `${workId}:${language.toLowerCase()}:${canonicalCodeDocumentUri(languageRootUri)}`;
}

function closeWorkspaceClient(key: string, entry: WorkspaceClientEntry) {
  if (workspaceClients.get(key) !== entry || entry.references > 0) return;
  workspaceClients.delete(key);
  entry.expectedClose = true;
  entry.disconnected = true;
  publishWorkspaceClientStatus(entry, {
    phase: "stopped",
    detail: "Language session released",
    progress: null,
    notice: null,
  });
  void entry.connection.then(({ client, close }) => {
    client.disconnect();
    close();
  }).catch(() => {});
}

function publishWorkspaceClientStatus(
  entry: WorkspaceClientEntry,
  status: CodeWorkspaceLspStatus,
) {
  entry.status = status;
  for (const listener of entry.listeners) listener(status);
}

function applyWorkspaceServerEvent(
  entry: WorkspaceClientEntry,
  event: CodeLanguageServerEvent,
) {
  if (event.kind === "progress") {
    publishWorkspaceClientStatus(entry, {
      ...entry.status,
      progress:
        event.progressKind === "end"
          ? null
          : {
              token: event.token,
              title: event.title || entry.status.progress?.title || "Language service",
              message: event.message,
              percentage: event.percentage,
            },
      notice: entry.status.notice,
    });
    return;
  }
  if (event.kind === "log" && (event.level === "error" || event.level === "warning")) {
    // rust-analyzer nags about workspace reload; Medousa owns the worktree — ignore.
    if (/auto-reloading is disabled/i.test(event.message)) return;
    publishWorkspaceClientStatus(entry, {
      ...entry.status,
      notice: event.message,
    });
  }
}

/** Suppress CodeMirror's top OK dialog for window/showMessage; route via onServerEvent. */
function quietShowMessageHandlers(
  onServerEvent?: (event: CodeLanguageServerEvent) => void,
): NonNullable<ConstructorParameters<typeof LSPClient>[0]>["notificationHandlers"] {
  return {
    "window/showMessage": (_client, params) => {
      const message =
        params && typeof (params as { message?: unknown }).message === "string"
          ? (params as { message: string }).message
          : "";
      const type = (params as { type?: unknown } | null)?.type;
      const level =
        type === 1
          ? "error"
          : type === 2
            ? "warning"
            : type === 3
              ? "info"
              : "log";
      if (message) {
        // Info spam (e.g. rust-analyzer auto-reload) stays out of the chrome.
        if (level === "info" || level === "log") return true;
        if (/auto-reloading is disabled/i.test(message)) return true;
        onServerEvent?.({ kind: "log", level, message });
      }
      return true;
    },
  };
}

function createWorkspaceClientEntry(
  key: string,
  options: {
    workId: string;
    workspaceRoot: string;
    language: string;
    documentUri: string;
    languageRootUri: string;
  },
): WorkspaceClientEntry {
  const workspaceBridge = new MedousaCodeWorkspaceBridge();
  let entry!: WorkspaceClientEntry;
  const connection = connectOrchestratorLspClient({
    ...options,
    workspaceBridge,
    onServerEvent: (event) => applyWorkspaceServerEvent(entry, event),
  });
  const client = connection.then(async (result) => {
    await result.ready;
    if (entry.disconnected) throw new Error("Language server disconnected while starting");
    publishWorkspaceClientStatus(entry, {
      phase: "ready",
      detail: `${options.language} language server ready`,
      progress: entry.status.progress,
      notice: entry.status.notice,
    });
    return result.client;
  });
  entry = {
    connection,
    client,
    workspaceBridge,
    references: 0,
    releaseTimer: null,
    status: {
      phase: "connecting",
      detail: `Starting ${options.language} language server`,
      progress: null,
      notice: null,
    },
    listeners: new Set(),
    expectedClose: false,
    disconnected: false,
  };
  void connection.then((result) => {
    void result.closed.then((closed) => {
      if (entry.expectedClose || closed.expected) return;
      entry.disconnected = true;
      if (workspaceClients.get(key) === entry) workspaceClients.delete(key);
      result.client.disconnect();
      result.close();
      const detail = closed.reason || `Language server connection closed (${closed.code || 1006})`;
      publishWorkspaceClientStatus(entry, {
        phase: "reconnecting",
        detail,
        progress: null,
        notice: null,
      });
    });
  });
  void client.catch((err) => {
    if (entry.expectedClose || entry.status.phase === "reconnecting") return;
    entry.disconnected = true;
    if (workspaceClients.get(key) === entry) workspaceClients.delete(key);
    publishWorkspaceClientStatus(entry, {
      phase: "failed",
      detail: err instanceof Error ? err.message : String(err),
      progress: null,
      notice: null,
    });
  });
  return entry;
}

function restartWorkspaceClient(key: string, entry: WorkspaceClientEntry) {
  if (workspaceClients.get(key) === entry) workspaceClients.delete(key);
  entry.expectedClose = true;
  entry.disconnected = true;
  publishWorkspaceClientStatus(entry, {
    phase: "reconnecting",
    detail: "Restarting language server",
    progress: null,
    notice: null,
  });
  void entry.connection.then(({ client, close }) => {
    client.disconnect();
    close();
  }).catch(() => {});
}

export async function acquireCodeWorkspaceLspClient(options: {
  workId: string;
  workspaceRoot: string;
  language: string;
  documentUri: string;
}): Promise<CodeWorkspaceLspLease> {
  const workspaceRoot = options.workspaceRoot.replace(/[\\/]+$/, "");
  const projectRootUri = canonicalCodeDocumentUri(pathToFileUri(workspaceRoot));
  let languageRootUri = projectRootUri;
  try {
    const resolved = await getCodeLanguageRoot({
      workId: options.workId,
      language: options.language,
      uri: options.documentUri,
    });
    languageRootUri =
      validatedCodeLanguageRootUri(resolved.root_uri, workspaceRoot) ?? projectRootUri;
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    // Permanent path/policy failures must not fall through to a websocket that
    // will immediately die and trigger reconnect spam.
    if (isPermanentLanguageServiceError(detail)) throw err;
    // Rolling-upgrade compatibility: older coding engines use the project root.
  }
  const key = codeWorkspaceLspPoolKey(
    options.workId,
    options.language,
    languageRootUri,
  );
  let entry = workspaceClients.get(key);
  if (!entry) {
    entry = createWorkspaceClientEntry(key, {
      workId: options.workId,
      workspaceRoot,
      language: options.language,
      documentUri: options.documentUri,
      languageRootUri,
    });
    workspaceClients.set(key, entry);
  }
  if (entry.releaseTimer) {
    clearTimeout(entry.releaseTimer);
    entry.releaseTimer = null;
  }
  entry.references += 1;
  let released = false;
  return {
    client: entry.client,
    workspaceBridge: entry.workspaceBridge,
    subscribeStatus(listener) {
      entry.listeners.add(listener);
      listener(entry.status);
      return () => entry.listeners.delete(listener);
    },
    restart() {
      restartWorkspaceClient(key, entry);
    },
    release() {
      if (released) return;
      released = true;
      entry.references = Math.max(0, entry.references - 1);
      if (entry.references === 0) {
        entry.releaseTimer = setTimeout(
          () => closeWorkspaceClient(key, entry),
          WORKSPACE_CLIENT_RELEASE_DELAY_MS,
        );
      }
    },
  };
}

if (typeof window !== "undefined") {
  window.addEventListener("beforeunload", () => {
    for (const [key, entry] of workspaceClients) {
      entry.references = 0;
      closeWorkspaceClient(key, entry);
    }
  });
}

async function codeAgentGet<T>(
  path: string,
  query: Record<string, string>,
): Promise<T> {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    const operations: Record<string, string> = {
      "/v1/code/language-root": "language_root",
      "/v1/code/language-sessions": "language_sessions",
      "/v1/code/language-matrix": "language_matrix",
      "/v1/code/symbols": "symbols",
      "/v1/code/workspace-symbols": "workspace_symbols",
      "/v1/code/workspace-diagnostics": "workspace_diagnostics",
      "/v1/code/capabilities": "capabilities",
      "/v1/code/conventions": "conventions",
    };
    const operation = operations[path];
    if (!operation) throw new Error(`Unsupported Code Intelligence read: ${path}`);
    return invoke<T>("code_read", { operation, query });
  }
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  const params = new URLSearchParams(query);
  const response = await fetch(`${base}${path}?${params}`, {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error((await response.text()) || response.statusText);
  }
  return (await response.json()) as T;
}

async function codeAgentPost<T>(
  path: string,
  body: Record<string, unknown>,
): Promise<T> {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    if (path !== "/v1/code/request") {
      throw new Error(`Unsupported Code Intelligence mutation: ${path}`);
    }
    return invoke<T>("code_request", { body });
  }
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  const response = await fetch(`${base}${path}`, {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error((await response.text()) || response.statusText);
  }
  return (await response.json()) as T;
}

export type CodeLanguageRoot = {
  language: string;
  root_uri: string;
  relative_root: string;
};

export async function getCodeLanguageRoot(options: {
  workId: string;
  uri: string;
  language: string;
}): Promise<CodeLanguageRoot> {
  const response = await codeAgentGet<{
    ok: boolean;
    language?: string;
    root_uri?: string;
    relative_root?: string;
  }>("/v1/code/language-root", {
    work_id: options.workId,
    uri: options.uri,
    language: options.language,
  });
  if (typeof response.root_uri !== "string" || !response.root_uri) {
    throw new Error("Coding engine returned an invalid language root");
  }
  return {
    language: response.language ?? options.language,
    root_uri: response.root_uri,
    relative_root: response.relative_root ?? "",
  };
}

export type CodeLanguageSessionLogEntry = {
  sequence: number;
  timestamp_ms: number;
  level: string;
  source: string;
  message: string;
};

export type CodeLanguageSessionProgress = {
  token: string;
  title: string;
  message: string;
  percentage?: number | null;
  done: boolean;
};

export type CodeLanguageSessionSnapshot = {
  id: string;
  kind: "editor" | "agent";
  language: string;
  project_root: string;
  language_root: string;
  relative_root: string;
  phase: "starting" | "initializing" | "ready" | "stopped" | "failed";
  detail: string;
  started_at_ms: number;
  updated_at_ms: number;
  progress: CodeLanguageSessionProgress[];
  logs: CodeLanguageSessionLogEntry[];
};

export async function getCodeLanguageSessions(options: {
  workId: string;
  uri: string;
  language: string;
}): Promise<{
  language: string;
  rootUri: string;
  sessions: CodeLanguageSessionSnapshot[];
}> {
  const response = await codeAgentGet<{
    ok: boolean;
    language?: string;
    root_uri?: string;
    sessions?: CodeLanguageSessionSnapshot[];
  }>("/v1/code/language-sessions", {
    work_id: options.workId,
    uri: options.uri,
    language: options.language,
  });
  return {
    language: response.language ?? options.language,
    rootUri: response.root_uri ?? "",
    sessions: Array.isArray(response.sessions) ? response.sessions : [],
  };
}

export type CodeLanguageMatrixEntry = {
  language: string;
  command: string | null;
  binaryAvailable: boolean;
  usable: boolean;
  packageId: string | null;
  rootMarkers: string[];
  extensions: string[];
  args: string[];
};

export async function getCodeLanguageMatrix(): Promise<CodeLanguageMatrixEntry[]> {
  const response = await codeAgentGet<{
    ok: boolean;
    languages?: Array<{
      language?: string;
      command?: string | null;
      binary_available?: boolean;
      usable?: boolean;
      package_id?: string | null;
      root_markers?: string[];
      extensions?: string[];
      args?: string[];
    }>;
  }>("/v1/code/language-matrix", {});
  if (!Array.isArray(response.languages)) return [];
  return response.languages
    .filter((entry) => typeof entry.language === "string" && entry.language)
    .map((entry) => ({
      language: entry.language!,
      command: typeof entry.command === "string" ? entry.command : null,
      binaryAvailable: Boolean(entry.binary_available ?? entry.usable),
      usable: Boolean(entry.usable ?? entry.binary_available),
      packageId: typeof entry.package_id === "string" ? entry.package_id : null,
      rootMarkers: Array.isArray(entry.root_markers)
        ? entry.root_markers.filter((marker): marker is string => typeof marker === "string")
        : [],
      extensions: Array.isArray(entry.extensions)
        ? entry.extensions.filter((ext): ext is string => typeof ext === "string")
        : [],
      args: Array.isArray(entry.args)
        ? entry.args.filter((arg): arg is string => typeof arg === "string")
        : [],
    }));
}

export function findCodeLanguageMatrixEntry(
  matrix: CodeLanguageMatrixEntry[],
  language: string,
): CodeLanguageMatrixEntry | null {
  const key = language.trim().toLowerCase();
  return matrix.find((entry) => entry.language.toLowerCase() === key) ?? null;
}

export type CodeDocumentSymbol = {
  name: string;
  kind?: number;
  range?: {
    start?: { line?: number; character?: number };
    end?: { line?: number; character?: number };
  };
  selectionRange?: {
    start?: { line?: number; character?: number };
    end?: { line?: number; character?: number };
  };
  children?: CodeDocumentSymbol[];
};

export async function getCodeDocumentSymbols(options: {
  workId: string;
  uri: string;
  language: string;
}): Promise<CodeDocumentSymbol[]> {
  const response = await codeAgentGet<{ ok: boolean; result?: CodeDocumentSymbol[] }>(
    "/v1/code/symbols",
    {
      work_id: options.workId,
      uri: options.uri,
      language: options.language,
    },
  );
  return Array.isArray(response.result) ? response.result : [];
}

export type CodeWorkspaceSymbol = {
  name: string;
  kind?: number;
  containerName?: string;
  location?: {
    uri?: string;
    range?: { start?: { line?: number; character?: number } };
  };
};

export async function getCodeWorkspaceSymbols(options: {
  workId: string;
  language: string;
  query: string;
  uri?: string;
}): Promise<CodeWorkspaceSymbol[]> {
  const query: Record<string, string> = {
    work_id: options.workId,
    language: options.language,
    query: options.query,
  };
  if (options.uri) query.uri = options.uri;
  const response = await codeAgentGet<{ ok: boolean; result?: CodeWorkspaceSymbol[] }>(
    "/v1/code/workspace-symbols",
    query,
  );
  return Array.isArray(response.result) ? response.result : [];
}

export type CodeWorkspaceDiagnostic = {
  uri?: string;
  language?: string;
  version?: number;
  diagnostics?: Array<{
    message?: string;
    severity?: number;
    source?: string;
    code?: string | number | { value?: string | number; target?: string };
    tags?: number[];
    range?: {
      start?: { line?: number; character?: number };
      end?: { line?: number; character?: number };
    };
    relatedInformation?: Array<{
      location?: {
        uri?: string;
        range?: {
          start?: { line?: number; character?: number };
          end?: { line?: number; character?: number };
        };
      };
      message?: string;
    }>;
  }>;
};

export type CodeWorkspaceDiagnosticsSnapshot = {
  scope?: "active_sessions" | "language" | "language_fallback" | string;
  languages: string[];
  documents: CodeWorkspaceDiagnostic[];
  unavailableLanguages?: string[];
};

export async function getCodeWorkspaceDiagnostics(options: {
  workId: string;
  language?: string;
}): Promise<CodeWorkspaceDiagnosticsSnapshot> {
  const query: Record<string, string> = { work_id: options.workId };
  if (options.language?.trim()) query.language = options.language.trim();
  const response = await codeAgentGet<{
    ok: boolean;
    scope?: string;
    languages?: string[];
    documents?: CodeWorkspaceDiagnostic[];
  }>("/v1/code/workspace-diagnostics", query);
  return {
    scope: response.scope,
    languages: Array.isArray(response.languages)
      ? response.languages.filter((language): language is string => typeof language === "string")
      : options.language
        ? [options.language]
        : [],
    documents: Array.isArray(response.documents) ? response.documents : [],
  };
}

/** New engines aggregate active sessions; older engines are queried per open language. */
export async function getAllCodeWorkspaceDiagnostics(options: {
  workId: string;
  languages: string[];
}): Promise<CodeWorkspaceDiagnosticsSnapshot> {
  let aggregateError: unknown = null;
  try {
    const aggregate = await getCodeWorkspaceDiagnostics({ workId: options.workId });
    if (aggregate.scope === "active_sessions") return aggregate;
  } catch (err) {
    aggregateError = err;
  }
  const languages = [...new Set(options.languages.map((language) => language.trim()).filter(Boolean))];
  const settled = await Promise.allSettled(
    languages.map((language) =>
      getCodeWorkspaceDiagnostics({ workId: options.workId, language }),
    ),
  );
  const snapshots = settled.flatMap((result) =>
    result.status === "fulfilled" ? [result.value] : [],
  );
  if (snapshots.length === 0 && aggregateError) throw aggregateError;
  const documents = new Map<string, CodeWorkspaceDiagnostic>();
  for (const snapshot of snapshots) {
    for (const document of snapshot.documents) {
      const key = `${document.language ?? ""}:${document.uri ?? ""}`;
      documents.set(key, document);
    }
  }
  return {
    scope: "language_fallback",
    languages,
    documents: [...documents.values()],
    unavailableLanguages: languages.filter(
      (_, index) => settled[index]?.status === "rejected",
    ),
  };
}

export type CodeLanguageCapabilities = Record<string, unknown>;

export async function getCodeLanguageCapabilities(options: {
  workId: string;
  uri: string;
  language: string;
}): Promise<CodeLanguageCapabilities> {
  const response = await codeAgentGet<{
    ok: boolean;
    capabilities?: CodeLanguageCapabilities;
  }>("/v1/code/capabilities", {
    work_id: options.workId,
    uri: options.uri,
    language: options.language,
  });
  return response.capabilities ?? {};
}

export type CodeEditorConventions = {
  indent_style?: "space" | "tab";
  indent_size?: string;
  tab_width?: string;
  end_of_line?: "lf" | "crlf" | "cr";
  insert_final_newline?: string;
};

export async function getCodeEditorConventions(options: {
  workId: string;
  uri: string;
  language: string;
}): Promise<CodeEditorConventions> {
  const response = await codeAgentGet<{
    ok: boolean;
    conventions?: CodeEditorConventions;
  }>("/v1/code/conventions", {
    work_id: options.workId,
    uri: options.uri,
    language: options.language,
  });
  return response.conventions ?? {};
}

export async function requestCodeLanguageAction(options: {
  workId: string;
  action: "references" | "rename" | "format" | "code_actions" | "organize_imports";
  uri: string;
  language: string;
  line?: number;
  character?: number;
  newName?: string;
  range?: unknown;
  diagnostics?: unknown[];
  editorOptions?: { tabSize: number; insertSpaces: boolean };
}): Promise<unknown> {
  const response = await codeAgentPost<{ ok: boolean; result?: unknown }>(
    "/v1/code/request",
    {
      work_id: options.workId,
      action: options.action,
      uri: options.uri,
      language: options.language,
      line: options.line,
      character: options.character,
      new_name: options.newName,
      range: options.range,
      diagnostics: options.diagnostics ?? [],
      options: options.editorOptions,
    },
  );
  return response.result;
}
