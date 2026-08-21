import { describe, expect, it, vi } from "vitest";
import { CodeProblemsController } from "./codeProblemsController.svelte";

function controller() {
  return new CodeProblemsController({
    getWorkId: () => "work-1",
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
