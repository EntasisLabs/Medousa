import type { Transport } from "@codemirror/lsp-client";
import {
  LSPClient,
  languageServerExtensions,
} from "@codemirror/lsp-client";
import {
  daemonWebSocketUrl,
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
    socket.onerror = () => reject(new Error("Grapheme LSP websocket failed"));
  });
}

export async function connectGraphemeLspClient(): Promise<{
  client: LSPClient;
  workspace: GraphemeLspWorkspaceResponse;
}> {
  const [wsUrl, workspace] = await Promise.all([
    daemonWebSocketUrl("/v1/grapheme/lsp"),
    getGraphemeLspWorkspace(),
  ]);
  const transport = await createWebSocketTransport(wsUrl);
  const client = new LSPClient({
    rootUri: workspace.root_uri,
    extensions: languageServerExtensions(),
  }).connect(transport);
  return { client, workspace };
}
