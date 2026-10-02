import { describe, expect, it, vi } from "vitest";
const diagnostics = vi.hoisted(() => vi.fn());
vi.mock("$lib/code/codingEngineClient", () => ({ getAllCodeWorkspaceDiagnostics: diagnostics }));
import { CodeProblemsController } from "./codeProblemsController.svelte";

function controller() {
  return new CodeProblemsController({
    getWorkId: () => "work-1",
    getScopeKey: () => "workshop-a/work-1",
    getWorkspaceRoot: () => "/work/project",
    getDocumentUri: () => "file:///work/project/src/main.rs",
    getActiveLanguage: () => "rust",
    getWorkspaceLanguages: () => ["rust"],
    persistPanel: vi.fn(),
    openProblem: vi.fn(async () => {}),
    onError: vi.fn(),
    syncDocument: vi.fn(),
  });
}

describe("task-backed Problems", () => {
  it("keeps task provenance alongside language diagnostics", () => {
    const problems = controller();
    problems.setDocumentProblems([{ message: "LSP issue", severity: "warning", line: 2 }]);
    problems.setTaskRun({
      runId: "run-2",
      taskLabel: "Cargo check",
      success: false,
      locations: [{ path: "src/main.rs", line: 9, column: 4, message: "cannot compile" }],
    });

    expect(problems.effective).toHaveLength(2);
    expect(problems.effective.find((problem) => problem.origin === "task")).toMatchObject({
      runId: "run-2",
      taskLabel: "Cargo check",
      path: "src/main.rs",
      line: 9,
      character: 4,
      fresh: true,
    });
  });

  it("replaces only the selected task run diagnostics", () => {
    const problems = controller();
    problems.setDocumentProblems([{ message: "LSP issue", severity: "error", line: 2 }]);
    problems.setTaskRun({
      runId: "run-1",
      taskLabel: "Build",
      success: false,
      locations: [{ path: "old.rs", line: 1, message: "old" }],
    });
    problems.setTaskRun({
      runId: "run-2",
      taskLabel: "Build",
      success: true,
      locations: [],
    });

    expect(problems.effective.map((problem) => problem.message)).toEqual(["LSP issue"]);
    expect(problems.taskRunId).toBe("run-2");
  });
});

it("does not populate another workshop with delayed diagnostics for the same project", async () => {
  let scope = "workshop-a";
  let resolve!: (value: unknown) => void;
  diagnostics.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const problems = new CodeProblemsController({
    getScopeKey: () => scope, getWorkId: () => "same-id", getWorkspaceRoot: () => "/repo",
    getDocumentUri: () => "file:///repo/app.ts", getActiveLanguage: () => "typescript",
    getWorkspaceLanguages: () => ["typescript"], persistPanel: vi.fn(), openProblem: vi.fn(), onError: vi.fn(), syncDocument: vi.fn(),
  });
  const pending = problems.refresh();
  scope = "workshop-b";
  problems.resetForScope();
  resolve({ documents: [{ uri: "file:///repo/app.ts", diagnostics: [{ message: "old issue", severity: 1 }] }] });
  await pending;
  expect(problems.workspaceProblems).toEqual([]);
  expect(problems.loaded).toBe(false);
  expect(problems.loading).toBe(false);
});

it("hides old document and task problems immediately when scope changes", () => {
  let scope = "workshop-a";
  const problems = new CodeProblemsController({
    getScopeKey: () => scope, getWorkId: () => "same-id", getWorkspaceRoot: () => "/repo",
    getDocumentUri: () => "file:///repo/app.ts", getActiveLanguage: () => "typescript",
    getWorkspaceLanguages: () => ["typescript"], persistPanel: vi.fn(), openProblem: vi.fn(), onError: vi.fn(), syncDocument: vi.fn(),
  });
  problems.setDocumentProblems([{ message: "old document", line: 1, severity: "error" }]);
  problems.setTaskRun({ runId: "old", taskLabel: "Check", success: false, locations: [{ path: "app.ts", line: 1, message: "old task" }] });
  expect(problems.effective).toHaveLength(2);
  scope = "workshop-b";
  expect(problems.effective).toEqual([]);
});

it("keeps current editor observations ahead of an older aggregate snapshot", async () => {
  const problems = controller();
  diagnostics.mockResolvedValueOnce({ scope: "active_sessions", languages: ["rust"], documents: [
    { uri: "file:///work/project/src/main.rs", language: "rust", diagnostics: [{ message: "older snapshot", range: { start: { line: 0, character: 0 }, end: { line: 0, character: 2 } } }] },
    { uri: "file:///work/project/src/other.rs", language: "rust", diagnostics: [{ message: "other file", range: { start: { line: 0, character: 0 }, end: { line: 0, character: 2 } } }] },
  ] });
  problems.setDocumentProblems([{ message: "current editor", severity: "warning", line: 1, character: 2, endCharacter: 4, source: "rust-analyzer" }]);
  await problems.refresh();
  expect(problems.effective.map((problem) => problem.message)).toEqual(["other file", "current editor"]);
  problems.setDocumentProblems([]);
  expect(problems.effective.map((problem) => problem.message)).toEqual(["other file"]);
});
