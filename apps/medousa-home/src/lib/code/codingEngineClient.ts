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
  daemonWebSocketUrl,
  getCodingEngineInfo,
  getDaemonUrl,
  getGraphemeLspWorkspace,
} from "$lib/daemon";
import type { GraphemeLspWorkspaceResponse } from "$lib/types/grapheme";

export function createWebSocketTransport(uri: string): Promise<Transport> {
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
      resolve({
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
      });
    };
    socket.onerror = () => reject(new Error("LSP websocket failed"));
  });
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

export async function connectOrchestratorLspClient(options?: {
  language?: string;
  workId?: string;
  workspaceRoot?: string;
}): Promise<{
  client: LSPClient;
  workspace: GraphemeLspWorkspaceResponse;
  via: "orchestrator" | "grapheme";
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
      const transport = await createWebSocketTransport(wsUrl);
      const rootUri = options?.workspaceRoot
        ? pathToFileUri(options.workspaceRoot)
        : info.workspace_root_uri || graphemeWorkspace.root_uri;
      const client = new LSPClient({
        rootUri,
        extensions: languageServerExtensions(),
      }).connect(transport);
      return {
        client,
        workspace: {
          ...graphemeWorkspace,
          root_uri: rootUri,
          root_path: info.workspace_root || graphemeWorkspace.root_path,
        },
        via: "orchestrator",
      };
    }
  } catch {
    if (language !== "grapheme") {
      throw new Error(`Smart editing is unavailable for ${language}`);
    }
    // Grapheme has an in-daemon fallback.
  }

  const wsUrl = await daemonWebSocketUrl("/v1/grapheme/lsp");
  const transport = await createWebSocketTransport(wsUrl);
  const client = new LSPClient({
    rootUri: graphemeWorkspace.root_uri,
    extensions: languageServerExtensions(),
  }).connect(transport);
  return { client, workspace: graphemeWorkspace, via: "grapheme" };
}

export function pathToFileUri(path: string): string {
  const normalized = path.replaceAll("\\", "/");
  const prefixed = normalized.startsWith("/") ? normalized : `/${normalized}`;
  return encodeURI(`file://${prefixed}`);
}

const workspaceClients = new Map<string, Promise<LSPClient>>();

export async function getCodeWorkspaceLspClient(options: {
  workId: string;
  workspaceRoot: string;
  language: string;
}): Promise<LSPClient> {
  const key = `${options.workId}:${options.language}`;
  const existing = workspaceClients.get(key);
  if (existing) return existing;
  const pending = connectOrchestratorLspClient(options)
    .then((result) => result.client)
    .catch((err) => {
      workspaceClients.delete(key);
      throw err;
    });
  workspaceClients.set(key, pending);
  return pending;
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
