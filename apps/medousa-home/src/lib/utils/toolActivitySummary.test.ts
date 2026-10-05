import { expect, it } from "vitest";
import type { ToolRunState } from "$lib/types/chat";
import { toolActivitySummary } from "./toolActivitySummary";

function run(toolName: string, status: ToolRunState["status"] = "succeeded"): ToolRunState {
  return { runId: toolName, toolName, status, round: 1 };
}

it("deduplicates activities without hiding any calls from the count", () => {
  expect(toolActivitySummary([
    run("turn.begin"), run("vault.read"), run("code.read_file"),
    run("code.apply_patch"), run("code.test"), run("turn.finish"),
  ])).toEqual({ countLabel: "6 tools", context: "Read · Edit · Test", running: 0, failed: 0 });
});

it("names the active activity while retaining a prior failure count", () => {
  expect(toolActivitySummary([run("git.status", "failed"), run("code.test", "running")]))
    .toEqual({ countLabel: "2 tools", context: "Running tests", running: 1, failed: 1 });
  expect(toolActivitySummary([run("turn.begin", "running")]).context).toBe("Running");
});

it("identifies the failed check without presenting the whole task as failed", () => {
  expect(toolActivitySummary([run("code.read"), run("git.status", "failed"), run("code.edit")]))
    .toEqual({ countLabel: "3 tools", context: "Git status", running: 0, failed: 1 });
});

it("does not imply tests ran when the tool only reads them or executes an unspecified command", () => {
  expect(toolActivitySummary([run("code.tests.read"), run("shell.exec")]).context).toBe("Read · Run");
});

it("keeps a bounded preview and useful names for unfamiliar tools", () => {
  expect(toolActivitySummary([run("web.search"), run("vault.read"), run("code.edit"), run("shell.exec")]).context)
    .toBe("Search · Read · Edit · …");
  expect(toolActivitySummary([run("mcp.weather.forecast")])).toEqual({
    countLabel: "1 tool", context: "Mcp weather forecast", running: 0, failed: 0,
  });
});
