import { describe, expect, it } from "vitest";
import { planAgentWorkspace } from "./agentWorkspacePlan";

describe("planAgentWorkspace", () => {
  it("waits for a project before starting an external coder", () => {
    expect(
      planAgentWorkspace({
        runtime: "codex",
        mode: "coder",
        bindingWorkId: null,
        agentSessionId: null,
        agentWorkId: undefined,
      }),
    ).toBe("wait_for_project");
  });

  it("starts external agents in the bound project", () => {
    expect(
      planAgentWorkspace({
        runtime: "cursor",
        mode: "coder",
        bindingWorkId: "work-a",
        agentSessionId: null,
        agentWorkId: undefined,
      }),
    ).toBe("start");
  });

  it("restarts an agent when the project changes or its launch binding is unknown", () => {
    for (const agentWorkId of ["work-a", undefined]) {
      expect(
        planAgentWorkspace({
          runtime: "codex",
          mode: "coder",
          bindingWorkId: "work-b",
          agentSessionId: "agent-1",
          agentWorkId,
        }),
      ).toBe("restart");
    }
  });

  it("keeps a plain external chat explicitly launched without a project", () => {
    for (const mode of ["instant", "general"] as const) {
      expect(
        planAgentWorkspace({
          runtime: "codex",
          mode,
          bindingWorkId: null,
          agentSessionId: "agent-1",
          agentWorkId: null,
        }),
      ).toBe("keep");
    }
  });
});
