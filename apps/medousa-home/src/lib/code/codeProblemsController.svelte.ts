/**
 * Workspace Problems + shared Code context-panel mode.
 * CodeSourceEditor wires the chrome; this owns filters, refresh, and panel mode.
 */

import {
  countCodeProblems,
  filterCodeProblems,
  groupCodeProblems,
  normalizeCodeWorkspaceProblems,
  type CodeProblem,
  type CodeProblemSeverityFilter,
} from "$lib/code/codeProblems";
import { getAllCodeWorkspaceDiagnostics } from "$lib/code/codingEngineClient";
import type { CodeContextPanel } from "$lib/code/codeWorkbenchState.svelte";

export type CodeEditorDocumentProblem = {
  message: string;
  severity: "error" | "warning" | "info" | "hint" | string;
  line: number;
};

export const PROBLEM_SEVERITY_OPTIONS: Array<{
  value: CodeProblemSeverityFilter;
  label: string;
}> = [
  { value: "all", label: "All" },
  { value: "error", label: "Errors" },
  { value: "warning", label: "Warnings" },
  { value: "information", label: "Info" },
];

export type CodeProblemsControllerDeps = {
  getWorkId: () => string;
  getWorkspaceRoot: () => string | null;
  getDocumentUri: () => string | null;
  getActiveLanguage: () => string;
  getWorkspaceLanguages: () => string[];
  persistPanel: (panel: CodeContextPanel) => void;
  openProblem: (problem: CodeProblem) => Promise<void>;
  onError: (message: string) => void;
  syncDocument: () => void;
};

function editorProblemsToWorkspace(
  problems: CodeEditorDocumentProblem[],
  documentUri: string,
  language: string,
  workspaceRoot: string,
): CodeProblem[] {
  return normalizeCodeWorkspaceProblems(
    [
      {
        uri: documentUri,
        language,
        diagnostics: problems.map((problem) => {
          const severity =
            problem.severity === "error"
              ? 1
              : problem.severity === "warning"
                ? 2
                : problem.severity === "info"
                  ? 3
                  : 4;
          return {
            message: problem.message,
            severity,
            range: {
              start: { line: Math.max(0, problem.line - 1), character: 0 },
              end: { line: Math.max(0, problem.line - 1), character: 0 },
            },
          };
        }),
      },
    ],
    workspaceRoot,
  );
}

export class CodeProblemsController {
  panel = $state<CodeContextPanel>(null);
  documentProblems = $state<CodeEditorDocumentProblem[]>([]);
  workspaceProblems = $state<CodeProblem[]>([]);
  workspaceScope = $state("");
  loaded = $state(false);
  loading = $state(false);
  error = $state<string | null>(null);
  unavailableLanguages = $state<string[]>([]);
  query = $state("");
  severity = $state<CodeProblemSeverityFilter>("all");
  #requestEpoch = 0;
  #deps: CodeProblemsControllerDeps;

  constructor(deps: CodeProblemsControllerDeps) {
    this.#deps = deps;
  }

  get scopeKey(): string {
    const workId = this.#deps.getWorkId();
    const root = this.#deps.getWorkspaceRoot();
    return workId && root ? `${workId}\u0000${root}` : "";
  }

  get documentFallback(): CodeProblem[] {
    const uri = this.#deps.getDocumentUri();
    const root = this.#deps.getWorkspaceRoot();
    if (!uri || !root) return [];
    return editorProblemsToWorkspace(
      this.documentProblems,
      uri,
      this.#deps.getActiveLanguage(),
      root,
    );
  }

  get effective(): CodeProblem[] {
    return this.loaded && this.workspaceScope === this.scopeKey
      ? this.workspaceProblems
      : this.documentFallback;
  }

  get filtered(): CodeProblem[] {
    return filterCodeProblems(this.effective, {
      query: this.query,
      severity: this.severity,
    });
  }

  get groups() {
    return groupCodeProblems(this.filtered);
  }

  get counts() {
    return countCodeProblems(this.effective);
  }

  setPanel(next: CodeContextPanel) {
    this.panel = next;
    this.#deps.persistPanel(next);
  }

  restorePanel(next: CodeContextPanel) {
    this.panel = next;
  }

  setDocumentProblems(next: CodeEditorDocumentProblem[]) {
    this.documentProblems = next;
  }

  async refresh(options?: { quiet?: boolean }) {
    const requestWorkId = this.#deps.getWorkId();
    const requestRoot = this.#deps.getWorkspaceRoot();
    const requestScope =
      requestWorkId && requestRoot ? `${requestWorkId}\u0000${requestRoot}` : "";
    const requestLanguages = [...this.#deps.getWorkspaceLanguages()];
    const requestEpoch = ++this.#requestEpoch;
    if (!requestScope || !requestRoot) {
      this.workspaceProblems = [];
      this.workspaceScope = "";
      this.loaded = false;
      this.loading = false;
      this.error = null;
      this.unavailableLanguages = [];
      return;
    }
    if (this.workspaceScope !== requestScope) {
      this.workspaceProblems = [];
      this.workspaceScope = requestScope;
      this.loaded = false;
      this.unavailableLanguages = [];
    }
    if (!options?.quiet || !this.loaded) this.loading = true;
    this.error = null;
    try {
      const snapshot = await getAllCodeWorkspaceDiagnostics({
        workId: requestWorkId,
        languages: requestLanguages,
      });
      if (requestEpoch !== this.#requestEpoch || this.scopeKey !== requestScope) {
        return;
      }
      this.workspaceProblems = normalizeCodeWorkspaceProblems(
        snapshot.documents,
        requestRoot,
      );
      this.unavailableLanguages = snapshot.unavailableLanguages ?? [];
      this.loaded = true;
    } catch (err) {
      if (requestEpoch !== this.#requestEpoch || this.scopeKey !== requestScope) {
        return;
      }
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      if (requestEpoch === this.#requestEpoch) this.loading = false;
    }
  }

  async openProblem(problem: CodeProblem) {
    this.#deps.onError("");
    try {
      await this.#deps.openProblem(problem);
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    }
  }

  async showProblems() {
    const next = this.panel === "problems" ? null : "problems";
    this.setPanel(next);
    if (next !== "problems") return;
    this.#deps.syncDocument();
  }
}
