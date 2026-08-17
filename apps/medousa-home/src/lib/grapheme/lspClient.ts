import type { Transport } from "@codemirror/lsp-client";
import {
  LSPClient,
  languageServerExtensions,
} from "@codemirror/lsp-client";
import {
  daemonWebSocketUrl,
  getGraphemeLspWorkspace,
  OPERATIONS,
} from "$lib/daemon";
import type { GraphemeLspWorkspaceResponse } from "$lib/types/grapheme";
import {
  connectOrchestratorLspClient,
  createWebSocketTransport,
} from "$lib/code/codingEngineClient";

export { createWebSocketTransport };

/** Connect via Orchestrator when available; otherwise Grapheme daemon LSP. */
export async function connectGraphemeLspClient(): Promise<{
  client: LSPClient;
  workspace: GraphemeLspWorkspaceResponse;
}> {
  const result = await connectOrchestratorLspClient({ language: "grapheme" });
  return { client: result.client, workspace: result.workspace };
}

export async function connectCodeLspClient(
  language = "grapheme",
  options?: { workId?: string; workspaceRoot?: string },
): Promise<{
  client: LSPClient;
  workspace: GraphemeLspWorkspaceResponse;
  via: "orchestrator" | "grapheme";
}> {
  return connectOrchestratorLspClient({ language, ...options });
}

export async function connectLegacyGraphemeOnly(): Promise<{
  client: LSPClient;
  workspace: GraphemeLspWorkspaceResponse;
}> {
  const [wsUrl, workspace] = await Promise.all([
    daemonWebSocketUrl(OPERATIONS["grapheme.lsp.get"].path),
    getGraphemeLspWorkspace(),
  ]);
  const transport = await createWebSocketTransport(wsUrl);
  const client = new LSPClient({
    rootUri: workspace.root_uri,
    extensions: languageServerExtensions(),
  }).connect(transport);
  return { client, workspace };
}

export type { Transport };
