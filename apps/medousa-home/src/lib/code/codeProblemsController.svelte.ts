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
import { getAllCodeWorkspaceDiagnostics, type CodeWorkspaceDiagnostic } from "$lib/code/codingEngineClient";
import type { CodeContextPanel } from "$lib/code/codeWorkbenchState.svelte";

export type CodeEditorDocumentProblem = {
  message: string;
  severity: "error" | "warning" | "info" | "hint" | string;
  line: number;
  character?: number;
  endLine?: number;
  endCharacter?: number;
  source?: string;
  code?: NonNullable<CodeWorkspaceDiagnostic["diagnostics"]>[number]["code"];
  tags?: number[];
  relatedInformation?: NonNullable<CodeWorkspaceDiagnostic["diagnostics"]>[number]["relatedInformation"];
  version?: number;
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
  getScopeKey: () => string;
  getWorkId: () => string;
  getWorkspaceRoot: () => string | null;
  getDocumentUri: () => string | null;
  getDocumentVersion?: () => number | null;
  getActiveLanguage: () => string;
  getWorkspaceLanguages: () => string[];
  persistPanel: (panel: CodeContextPanel) => void;
  openProblem: (problem: CodeProblem) => Promise<void>;
  onError: (message: string) => void;
  syncDocument: () => void;
  onProblemsSelected?: (selected: boolean) => void;
};

export type CodeTaskProblemRun = {
  runId: string;
  taskLabel: string;
  success: boolean | null;
  locations: Array<{
    path: string;
    line: number;
    column?: number | null;
    message: string;
  }>;
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
        version: problems[0]?.version,
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
            source: problem.source, code: problem.code, tags: problem.tags, relatedInformation: problem.relatedInformation,
            severity,
            range: {
              start: { line: Math.max(0, problem.line - 1), character: Math.max(0, (problem.character ?? 1) - 1) },
              end: { line: Math.max(0, (problem.endLine ?? problem.line) - 1), character: Math.max(0, (problem.endCharacter ?? problem.character ?? 1) - 1) },
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
  taskProblems = $state<CodeProblem[]>([]);
  taskRunId = $state<string | null>(null);
  workspaceScope = $state("");
  loaded = $state(false);
  loading = $state(false);
  error = $state<string | null>(null);
  unavailableLanguages = $state<string[]>([]);
  observedDocuments = $state(0);
  analysisScope = $state<string | null>(null);
  query = $state("");
  severity = $state<CodeProblemSeverityFilter>("all");
  #requestEpoch = 0;
  #documentScope = "";
  #documentUri: string | null = null;
  #taskScope = "";
  #deps: CodeProblemsControllerDeps;

  constructor(deps: CodeProblemsControllerDeps) {
    this.#deps = deps;
  }

  get scopeKey(): string {
    const workId = this.#deps.getWorkId();
    const root = this.#deps.getWorkspaceRoot();
    return workId && root ? this.#deps.getScopeKey() : "";
  }

  resetForScope() {
    this.#requestEpoch += 1;
    this.documentProblems = [];
    this.workspaceProblems = [];
    this.taskProblems = [];
    this.taskRunId = null;
    this.workspaceScope = "";
    this.loaded = false;
    this.loading = false;
    this.error = null;
    this.unavailableLanguages = [];
    this.observedDocuments = 0;
    this.analysisScope = null;
    this.#documentScope = "";
    this.#documentUri = null;
    this.#taskScope = "";
  }

  get documentFallback(): CodeProblem[] {
    const uri = this.#deps.getDocumentUri();
    const root = this.#deps.getWorkspaceRoot();
    if (!uri || !root || this.#documentScope !== this.scopeKey || this.#documentUri !== uri) return [];
    return editorProblemsToWorkspace(
      this.documentProblems,
      uri,
      this.#deps.getActiveLanguage(),
      root,
    );
  }

  get effective(): CodeProblem[] {
    const languageProblems = this.loaded && this.workspaceScope === this.scopeKey
      ? this.workspaceProblems
      : this.documentFallback;
    const uri = this.#deps.getDocumentUri();
    const version = this.#deps.getDocumentVersion?.();
    const observations = languageProblems.map((problem) => problem.uri === uri && version != null ? { ...problem, fresh: problem.documentVersion != null && problem.documentVersion === version } : problem);
    return [...observations, ...(this.#taskScope === this.scopeKey ? this.taskProblems : [])];
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
    this.#deps.onProblemsSelected?.(next === "problems");
  }

  restorePanel(next: CodeContextPanel) {
    this.panel = next;
  }

  setDocumentProblems(next: CodeEditorDocumentProblem[]) {
    this.#documentScope = this.scopeKey;
    this.#documentUri = this.#deps.getDocumentUri();
    this.documentProblems = next;
  }

  setTaskRun(run: CodeTaskProblemRun | null) {
    this.#taskScope = this.scopeKey;
    if (!run) {
      this.taskRunId = null;
      this.taskProblems = [];
      return;
    }
    this.taskRunId = run.runId;
    this.taskProblems = run.locations.map((location, index) => ({
      id: `task\u0000${run.runId}\u0000${location.path}\u0000${location.line}\u0000${location.column ?? 1}\u0000${location.message}\u0000${index}`,
      uri: `task-run://${run.runId}/${location.path}`,
      path: location.path,
      language: "",
      message: location.message || `Task reported a problem in ${location.path}`,
      severity: "error",
      severityNumber: 1,
      line: Math.max(1, location.line),
      character: Math.max(1, location.column ?? 1),
      endLine: Math.max(1, location.line),
      endCharacter: Math.max(1, location.column ?? 1),
      source: run.taskLabel,
      tags: [],
      relatedInformation: [],
      origin: "task",
      runId: run.runId,
      taskLabel: run.taskLabel,
      fresh: true,
    }));
  }

  async refresh(options?: { quiet?: boolean }) {
    const requestWorkId = this.#deps.getWorkId();
    const requestRoot = this.#deps.getWorkspaceRoot();
    const requestScope = this.scopeKey;
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
      this.observedDocuments = snapshot.documents.length;
      this.analysisScope = snapshot.scope ?? null;
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
