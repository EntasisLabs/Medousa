import type { Transport } from "@codemirror/lsp-client";

export type CodeLanguageServiceIdentity = {
  language: string;
  rootUri: string;
  path: string;
  serverName: string | null;
  serverVersion: string | null;
};

/** Keep optional provider identity from the matching initialization response. */
export function observeCodeLanguageService(
  transport: Transport,
  context: Pick<CodeLanguageServiceIdentity, "language" | "rootUri" | "path">,
) {
  const service: CodeLanguageServiceIdentity = { ...context, serverName: null, serverVersion: null };
  let initializeId: unknown;
  const observe = (raw: string) => {
    try {
      const response = JSON.parse(raw);
      if (response.id !== initializeId) return;
      const server = response.result?.serverInfo;
      if (typeof server?.name === "string") service.serverName = server.name;
      if (typeof server?.version === "string") service.serverVersion = server.version;
    } catch {
      // The LSP client handles protocol parsing and errors.
    }
  };
  transport.subscribe(observe);
  return {
    service,
    transport: {
      send(raw: string) {
        const message = JSON.parse(raw);
        if (message.method === "initialize") initializeId = message.id;
        transport.send(raw);
      },
      subscribe: (handler: (raw: string) => void) => transport.subscribe(handler),
      unsubscribe: (handler: (raw: string) => void) => transport.unsubscribe(handler),
    } satisfies Transport,
    validate() {
      if (service.serverName?.toLowerCase() === "grapheme-lsp" && service.language !== "grapheme") {
        throw new Error(`Unexpected language server grapheme-lsp for ${service.language}`);
      }
    },
    dispose: () => transport.unsubscribe(observe),
  };
}
