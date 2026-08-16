/**
 * Quick Open (⌘P): fuzzy files, @workspace symbols, and :line.
 * CodeSourceEditor wires open/reveal; this owns picker state and actions.
 */

import {
  getUndertakingSourceTree,
  type ForgeSourceTreeFile,
} from "$lib/code/codeDocumentService";
import type { CodeWorkspaceSymbol } from "$lib/code/codingEngineClient";
import { fuzzyMatchPaths } from "$lib/utils/pathFuzzyMatch";

export type CodeQuickOpenMode = "file" | "symbol" | "line";

export type CodeQuickOpenLspClient = {
  sync: () => void;
  request: <Params, Result>(method: string, params: Params) => Promise<Result>;
};

export type CodeQuickOpenControllerDeps = {
  getWorkId: () => string;
  getLspClient: () => CodeQuickOpenLspClient | null;
  pathFromUri: (uri?: string) => string | null;
  onError: (message: string) => void;
  onShown: () => void;
  openFile: (path: string, line: number) => Promise<void>;
  revealLine: (line: number) => void;
};

export class CodeQuickOpenController {
  open = $state(false);
  query = $state("");
  files = $state<ForgeSourceTreeFile[]>([]);
  symbols = $state<CodeWorkspaceSymbol[]>([]);
  symbolQuery = $state("");
  loading = $state(false);
  index = $state(0);

  #deps: CodeQuickOpenControllerDeps;

  constructor(deps: CodeQuickOpenControllerDeps) {
    this.#deps = deps;
  }

  get mode(): CodeQuickOpenMode {
    if (this.query.startsWith("@")) return "symbol";
    if (this.query.startsWith(":")) return "line";
    return "file";
  }

  get fileResults(): ForgeSourceTreeFile[] {
    const needle = this.query.trim().replace(/^>/, "");
    return fuzzyMatchPaths(this.files, needle, 80);
  }

  get symbolResults(): CodeWorkspaceSymbol[] {
    return this.symbols.slice(0, 80);
  }

  get resultCount(): number {
    if (this.mode === "symbol") return this.symbolResults.length;
    if (this.mode === "line") return 1;
    return this.fileResults.length;
  }

  close() {
    this.open = false;
  }

  setFiles(files: ForgeSourceTreeFile[]) {
    this.files = files;
  }

  async refreshTree() {
    const workId = this.#deps.getWorkId();
    if (!workId) return;
    try {
      this.files = (await getUndertakingSourceTree(workId)).files;
    } catch {
      /* tree refresh can retry later */
    }
  }

  async show() {
    this.open = true;
    this.query = "";
    this.index = 0;
    this.#deps.onShown();
    if (this.files.length || !this.#deps.getWorkId()) return;
    this.loading = true;
    try {
      await this.refreshTree();
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      this.loading = false;
    }
  }

  async refreshSymbols() {
    const query = this.query.startsWith("@") ? this.query.slice(1).trim() : "";
    const client = this.#deps.getLspClient();
    if (!this.#deps.getWorkId() || this.mode !== "symbol" || !client) return;
    this.symbolQuery = query;
    try {
      client.sync();
      const result = await client.request<{ query: string }, CodeWorkspaceSymbol[] | null>(
        "workspace/symbol",
        { query },
      );
      if (this.symbolQuery === query) this.symbols = Array.isArray(result) ? result : [];
    } catch {
      if (this.symbolQuery === query) this.symbols = [];
    }
  }

  onQueryInput() {
    this.index = 0;
    void this.refreshSymbols();
  }

  moveIndex(delta: number) {
    this.index = Math.max(0, Math.min(this.index + delta, this.resultCount - 1));
  }

  async chooseFile(file = this.fileResults[this.index]) {
    if (!file) return;
    this.open = false;
    await this.#deps.openFile(file.path, 1);
  }

  async chooseSymbol(symbol = this.symbolResults[this.index]) {
    const path = this.#deps.pathFromUri(symbol?.location?.uri);
    if (!symbol || !path) return;
    const line = (symbol.location?.range?.start?.line ?? 0) + 1;
    this.open = false;
    await this.#deps.openFile(path, line);
  }

  chooseLine() {
    const line = Number.parseInt(this.query.slice(1).trim(), 10);
    if (!Number.isFinite(line) || line < 1) return;
    this.open = false;
    this.#deps.revealLine(line);
  }

  chooseResult() {
    if (this.mode === "symbol") void this.chooseSymbol();
    else if (this.mode === "line") this.chooseLine();
    else void this.chooseFile();
  }
}
