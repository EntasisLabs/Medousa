/** Document-scoped language observations; late replies never follow the editor. */
import type { LSPClient } from "@codemirror/lsp-client";
import { getCodeEditorConventions, type CodeDocumentSymbol } from "$lib/code/codingEngineClient";
import { captureCodeScope } from "$lib/code/codeWorkspaceContext.svelte";
import { deferCodeWorkspaceWork } from "$lib/utils/codeWorkspaceTrace";

export class CodeLanguageInsights {
  symbols = $state<CodeDocumentSymbol[]>([]);
  symbolsLoading = $state(false);
  capabilities = $state<Record<string, unknown>>({});
  conventions = $state<{ indent_style?: "space" | "tab"; indent_size?: string; tab_width?: string }>({});
  #symbolEpoch = 0;

  constructor(private readonly deps: {
    getScopeKey: () => string;
    getDocumentUri: () => string | null;
    getWorkId: () => string;
    getLanguage: () => string;
    getClient: () => LSPClient | null;
    onError: (message: string) => void;
  }) {}

  reset() {
    this.#symbolEpoch += 1;
    this.symbols = [];
    this.symbolsLoading = false;
    this.capabilities = {};
    this.conventions = {};
  }

  #capture(client: LSPClient) {
    const current = captureCodeScope(() => JSON.stringify([
      this.deps.getScopeKey(), this.deps.getDocumentUri(),
    ]));
    return () => current() && this.deps.getClient() === client;
  }

  async refreshSymbols() {
    const client = this.deps.getClient();
    const uri = this.deps.getDocumentUri();
    if (!client || !uri) { this.symbols = []; return; }
    const current = this.#capture(client);
    const epoch = ++this.#symbolEpoch;
    this.symbolsLoading = true;
    try {
      client.sync();
      const result = await client.request<{ textDocument: { uri: string } }, CodeDocumentSymbol[] | null>(
        "textDocument/documentSymbol", { textDocument: { uri } },
      );
      if (current() && epoch === this.#symbolEpoch) this.symbols = Array.isArray(result) ? result : [];
    } catch (err) {
      if (current() && epoch === this.#symbolEpoch) {
        this.deps.onError(err instanceof Error ? err.message : String(err));
        this.symbols = [];
      }
    } finally {
      if (current() && epoch === this.#symbolEpoch) this.symbolsLoading = false;
    }
  }

  bind(interactive: boolean): () => void {
    this.reset();
    const client = this.deps.getClient();
    const uri = this.deps.getDocumentUri();
    const workId = this.deps.getWorkId();
    const language = this.deps.getLanguage();
    if (!interactive || !client || !uri) return () => {};
    const current = this.#capture(client);
    let cancelled = false;
    const cancelDeferred = deferCodeWorkspaceWork(() => {
      if (cancelled || !current()) return;
      void this.refreshSymbols();
      this.capabilities = (client.serverCapabilities ?? {}) as Record<string, unknown>;
      void getCodeEditorConventions({ workId, uri, language }).then((conventions) => {
        if (!cancelled && current()) this.conventions = conventions;
      }).catch(() => {
        if (!cancelled && current()) this.conventions = {};
      });
    });
    return () => { cancelled = true; cancelDeferred(); };
  }
}
