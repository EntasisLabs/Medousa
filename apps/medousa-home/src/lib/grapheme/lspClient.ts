import type { Transport } from "@codemirror/lsp-client";
import {
  connectOrchestratorLspClient,
  createWebSocketTransport,
} from "$lib/code/codingEngineClient";

export { createWebSocketTransport };

/** Grapheme uses the same required coding-engine route as other languages. */
export function connectGraphemeLspClient() {
  return connectOrchestratorLspClient({ language: "grapheme" });
}

export function connectCodeLspClient(
  language: string,
  options?: { workId?: string; workspaceRoot?: string },
) {
  return connectOrchestratorLspClient({ language, ...options });
}

export type { Transport };
