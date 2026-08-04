import { describe, expect, it } from "vitest";
import { buildCodeIntentContext, relativePathWithin } from "./coderContext.js";

describe("VS Code Coder context", () => {
  it("only emits paths inside the Forge worktree", () => {
    expect(relativePathWithin("/forge/work-1", "/forge/work-1/src/main.ts"))
      .toBe("src/main.ts");
    expect(relativePathWithin("/forge/work-1", "/repos/original/src/main.ts"))
      .toBeUndefined();
  });

  it("keeps the undertaking authoritative while carrying editor observations", () => {
    const result = buildCodeIntentContext(
      {
        surface: "vscode",
        file: "/forge/work-1/src/main.ts",
        cursor: { line: 9, character: 4 },
        selection: {
          text: "const answer = 42;",
          start: { line: 9, character: 0 },
          end: { line: 9, character: 18 },
        },
        diagnostics: [{ severity: "warning", source: "ts", message: "Unused" }],
      },
      {
        id: "work-1",
        title: "Compiler",
        brief: "Fix lowering",
        state: "ready",
        human_phase: "work",
        environment: {
          worktree: "/forge/work-1",
          branch: "medousa/attempt/work-1/1",
          baseline_oid: "abc",
          generation: 1,
        },
      },
      ["/forge/work-1/src/main.ts", "/repos/original/README.md"],
    );

    expect(result.work_id).toBe("work-1");
    expect(result.active_path).toBe("src/main.ts");
    expect(result.cursor_line).toBe(10);
    expect(result.open_files).toEqual(["src/main.ts"]);
    expect(result.diagnostics).toEqual(["warning · ts · Unused"]);
  });
});
