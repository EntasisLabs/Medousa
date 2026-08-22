import { workspaceRelativePathFromUri } from "./codeDocumentUri";
import type { CodeWorkspaceDiagnostic } from "./codingEngineClient";

export type CodeProblemSeverity = "error" | "warning" | "information" | "hint";
export type CodeProblemSeverityFilter = "all" | "error" | "warning" | "information";

export type CodeProblem = {
  id: string;
  uri: string;
  path: string;
  language: string;
  message: string;
  severity: CodeProblemSeverity;
  severityNumber: 1 | 2 | 3 | 4;
  line: number;
  character: number;
  endLine: number;
  endCharacter: number;
  source?: string;
  code?: string;
  tags: number[];
  relatedInformation: Array<{
    uri: string;
    path: string | null;
    line: number;
    character: number;
    message: string;
  }>;
  /** Diagnostic producer; task diagnostics carry run provenance separately from LSP. */
  origin: "language" | "task";
  runId?: string;
  taskLabel?: string;
  fresh?: boolean;
};

export type CodeProblemCounts = {
  total: number;
  errors: number;
  warnings: number;
  information: number;
  hints: number;
};

export type CodeProblemGroup = {
  path: string;
  problems: CodeProblem[];
  counts: CodeProblemCounts;
};

function positionNumber(value: unknown): number {
  return Number.isInteger(value) && (value as number) >= 0 ? (value as number) : 0;
}

function severityNumber(value: unknown): 1 | 2 | 3 | 4 {
  return value === 2 || value === 3 || value === 4 ? value : 1;
}

function severityName(value: 1 | 2 | 3 | 4): CodeProblemSeverity {
  if (value === 2) return "warning";
  if (value === 3) return "information";
  if (value === 4) return "hint";
  return "error";
}

function diagnosticCode(value: unknown): string | undefined {
  if (typeof value === "string" || typeof value === "number") return String(value);
  if (value && typeof value === "object" && "value" in value) {
    const nested = (value as { value?: unknown }).value;
    if (typeof nested === "string" || typeof nested === "number") return String(nested);
  }
  return undefined;
}

export function countCodeProblems(problems: CodeProblem[]): CodeProblemCounts {
  return problems.reduce<CodeProblemCounts>(
    (counts, problem) => {
      counts.total += 1;
      if (problem.severity === "error") counts.errors += 1;
      else if (problem.severity === "warning") counts.warnings += 1;
      else if (problem.severity === "information") counts.information += 1;
      else counts.hints += 1;
      return counts;
    },
    { total: 0, errors: 0, warnings: 0, information: 0, hints: 0 },
  );
}

export function normalizeCodeWorkspaceProblems(
  documents: CodeWorkspaceDiagnostic[],
  workspaceRoot: string,
): CodeProblem[] {
  const problems = new Map<string, CodeProblem>();
  for (const document of documents) {
    if (typeof document.uri !== "string") continue;
    const path = workspaceRelativePathFromUri(document.uri, workspaceRoot);
    if (!path || !Array.isArray(document.diagnostics)) continue;
    const language = typeof document.language === "string" ? document.language : "";
    for (const diagnostic of document.diagnostics) {
      if (!diagnostic || typeof diagnostic.message !== "string" || !diagnostic.message.trim()) {
        continue;
      }
      const severity = severityNumber(diagnostic.severity);
      const startLine = positionNumber(diagnostic.range?.start?.line);
      const startCharacter = positionNumber(diagnostic.range?.start?.character);
      const endLine = positionNumber(diagnostic.range?.end?.line ?? startLine);
      const endCharacter = positionNumber(
        diagnostic.range?.end?.character ?? startCharacter,
      );
      const source = typeof diagnostic.source === "string" && diagnostic.source.trim()
        ? diagnostic.source.trim()
        : undefined;
      const code = diagnosticCode(diagnostic.code);
      const key = [
        language,
        document.uri,
        startLine,
        startCharacter,
        endLine,
        endCharacter,
        severity,
        source ?? "",
        code ?? "",
        diagnostic.message,
      ].join("\u0000");
      if (problems.has(key)) continue;
      problems.set(key, {
        id: key,
        uri: document.uri,
        path,
        language,
        message: diagnostic.message,
        severity: severityName(severity),
        severityNumber: severity,
        line: startLine + 1,
        character: startCharacter + 1,
        endLine: endLine + 1,
        endCharacter: endCharacter + 1,
        source,
        code,
        tags: Array.isArray(diagnostic.tags)
          ? diagnostic.tags.filter((tag): tag is number => Number.isInteger(tag))
          : [],
        relatedInformation: Array.isArray(diagnostic.relatedInformation)
          ? diagnostic.relatedInformation.flatMap((related) => {
              const uri = related.location?.uri;
              if (typeof uri !== "string" || typeof related.message !== "string") return [];
              return [{
                uri,
                path: workspaceRelativePathFromUri(uri, workspaceRoot),
                line: positionNumber(related.location?.range?.start?.line) + 1,
                character: positionNumber(related.location?.range?.start?.character) + 1,
                message: related.message,
              }];
            })
          : [],
        origin: "language",
      });
    }
  }
  return [...problems.values()].sort(
    (left, right) =>
      left.severityNumber - right.severityNumber ||
      left.path.localeCompare(right.path) ||
      left.line - right.line ||
      left.character - right.character ||
      left.message.localeCompare(right.message),
  );
}

export function filterCodeProblems(
  problems: CodeProblem[],
  options: { query?: string; severity?: CodeProblemSeverityFilter },
): CodeProblem[] {
  const query = options.query?.trim().toLocaleLowerCase() ?? "";
  const severity = options.severity ?? "all";
  return problems.filter((problem) => {
    if (severity === "error" && problem.severity !== "error") return false;
    if (severity === "warning" && problem.severity !== "warning") return false;
    if (
      severity === "information" &&
      problem.severity !== "information" &&
      problem.severity !== "hint"
    ) return false;
    if (!query) return true;
    return [problem.path, problem.message, problem.source, problem.code, problem.language]
      .filter(Boolean)
      .some((value) => value!.toLocaleLowerCase().includes(query));
  });
}

export function groupCodeProblems(problems: CodeProblem[]): CodeProblemGroup[] {
  const grouped = new Map<string, CodeProblem[]>();
  for (const problem of problems) {
    const rows = grouped.get(problem.path) ?? [];
    rows.push(problem);
    grouped.set(problem.path, rows);
  }
  return [...grouped]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([path, rows]) => ({ path, problems: rows, counts: countCodeProblems(rows) }));
}
