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
      const path = `${info.daemon_lsp_path || "/v1/code/lsp"}?language=${encodeURIComponent(language)}`;
      const wsUrl = await daemonWebSocketUrl(path);
      const transport = await createWebSocketTransport(wsUrl);
      const rootUri = info.workspace_root_uri || graphemeWorkspace.root_uri;
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
    // Fall through to Grapheme daemon LSP.
  }

  const wsUrl = await daemonWebSocketUrl("/v1/grapheme/lsp");
  const transport = await createWebSocketTransport(wsUrl);
  const client = new LSPClient({
    rootUri: graphemeWorkspace.root_uri,
    extensions: languageServerExtensions(),
  }).connect(transport);
  return { client, workspace: graphemeWorkspace, via: "grapheme" };
}
