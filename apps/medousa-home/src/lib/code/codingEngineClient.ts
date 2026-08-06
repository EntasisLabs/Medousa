/**
 * Discover + connect to the workshop LSP Interoperability Orchestrator.
 * Prefer daemon-proxied `/v1/code/lsp` so remote Connection works.
 * Falls back to in-daemon Grapheme LSP when the coding engine is unavailable.
 */

import type { Transport } from "@codemirror/lsp-client";
import {
  LSPClient,
  languageServerExtensions,
} from "@codemirror/lsp-client";
import {
  MedousaCodeWorkspace,
  MedousaCodeWorkspaceBridge,
} from "$lib/code/medousaCodeWorkspace";
import { pathToFileUri } from "$lib/code/codeDocumentUri";

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
};

function createCloseableWebSocketTransport(uri: string): Promise<CloseableTransport> {
  const handlers: Array<(value: string) => void> = [];
  const socket = new WebSocket(uri);
  socket.onmessage = (event) => {
    const payload =
      typeof event.data === "string" ? event.data : event.data.toString();
    for (const handler of handlers) {
      handler(payload);
    }
  };
  return new Promise((resolve, reject) => {
    socket.onopen = () => {
      const transport: Transport = {
        send(message: string) {
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
        close() {
          handlers.length = 0;
          socket.onmessage = null;
          if (socket.readyState === WebSocket.CONNECTING || socket.readyState === WebSocket.OPEN) {
            socket.close(1000, "workspace released");
          }
        },
      });
    };
    socket.onerror = () => reject(new Error("LSP websocket failed"));
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
  workspaceRoot?: string;
  workspaceBridge?: MedousaCodeWorkspaceBridge;
};

export async function connectOrchestratorLspClient(options?: ConnectOrchestratorLspOptions): Promise<{
  client: LSPClient;
  workspace: GraphemeLspWorkspaceResponse;
  via: "orchestrator" | "grapheme";
  close: () => void;
  ready: Promise<null>;
}> {
  const language = (options?.language ?? "grapheme").trim() || "grapheme";
  const graphemeWorkspace = await getGraphemeLspWorkspace();

  try {
    const info = await getCodingEngineInfo();
    if (info.available) {
      const query = new URLSearchParams({ language });
      if (options?.workId) query.set("work_id", options.workId);
      const path = `${info.daemon_lsp_path || "/v1/code/lsp"}?${query}`;
      const wsUrl = await daemonWebSocketUrl(path);
      const connection = await createCloseableWebSocketTransport(wsUrl);
      const rootUri = options?.workspaceRoot
        ? pathToFileUri(options.workspaceRoot)
        : info.workspace_root_uri || graphemeWorkspace.root_uri;
      const client = new LSPClient({
        rootUri,
        timeout: 30_000,
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
        workspace: {
          ...graphemeWorkspace,
          root_uri: rootUri,
          root_path: info.workspace_root || graphemeWorkspace.root_path,
        },
        via: "orchestrator",
        close: connection.close,
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
  const connection = await createCloseableWebSocketTransport(wsUrl);
  const client = new LSPClient({
    rootUri: graphemeWorkspace.root_uri,
    timeout: 30_000,
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
    ready,
  };
}

type WorkspaceClientEntry = {
  connection: ReturnType<typeof connectOrchestratorLspClient>;
  client: Promise<LSPClient>;
  workspaceBridge: MedousaCodeWorkspaceBridge;
  references: number;
  releaseTimer: ReturnType<typeof setTimeout> | null;
};

export type CodeWorkspaceLspLease = {
  client: Promise<LSPClient>;
  workspaceBridge: MedousaCodeWorkspaceBridge;
  release: () => void;
};

const WORKSPACE_CLIENT_RELEASE_DELAY_MS = 1_000;
const workspaceClients = new Map<string, WorkspaceClientEntry>();

function closeWorkspaceClient(key: string, entry: WorkspaceClientEntry) {
  if (workspaceClients.get(key) !== entry || entry.references > 0) return;
  workspaceClients.delete(key);
  void entry.connection.then(({ client, close }) => {
    client.disconnect();
    close();
  }).catch(() => {});
}

export function acquireCodeWorkspaceLspClient(options: {
  workId: string;
  workspaceRoot: string;
  language: string;
}): CodeWorkspaceLspLease {
  const workspaceRoot = options.workspaceRoot.replace(/[\\/]+$/, "");
  const key = `${options.workId}:${options.language.toLowerCase()}:${workspaceRoot}`;
  let entry = workspaceClients.get(key);
  if (!entry) {
    const workspaceBridge = new MedousaCodeWorkspaceBridge();
    const connection = connectOrchestratorLspClient({ ...options, workspaceBridge })
    .catch((err) => {
      if (workspaceClients.get(key)?.connection === connection) {
        workspaceClients.delete(key);
      }
      throw err;
    });
    const client = connection
      .then(async (result) => {
        await result.ready;
        return result.client;
      })
      .catch((err) => {
        workspaceClients.delete(key);
        throw err;
      });
    entry = {
      connection,
      client,
      workspaceBridge,
      references: 0,
      releaseTimer: null,
    };
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
}): Promise<CodeWorkspaceSymbol[]> {
  const response = await codeAgentGet<{ ok: boolean; result?: CodeWorkspaceSymbol[] }>(
    "/v1/code/workspace-symbols",
    {
      work_id: options.workId,
      language: options.language,
      query: options.query,
    },
  );
  return Array.isArray(response.result) ? response.result : [];
}

export type CodeWorkspaceDiagnostic = {
  uri?: string;
  diagnostics?: Array<{
    message?: string;
    severity?: number;
    range?: { start?: { line?: number; character?: number } };
  }>;
};

export async function getCodeWorkspaceDiagnostics(options: {
  workId: string;
  language: string;
}): Promise<CodeWorkspaceDiagnostic[]> {
  const response = await codeAgentGet<{
    ok: boolean;
    documents?: CodeWorkspaceDiagnostic[];
  }>("/v1/code/workspace-diagnostics", {
    work_id: options.workId,
    language: options.language,
  });
  return Array.isArray(response.documents) ? response.documents : [];
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
