import { afterEach, expect, it } from "vitest";
import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
import { setCoderExecutionTransport } from "$lib/executionAuthority";
import { codeWorkspaceScopeKey, codeExecutionScopeKey, captureCodeScope, invalidateCodeWorkshopContext } from "./codeWorkspaceContext.svelte";

afterEach(() => {
  setActiveWorkshopIdPort(null);
  setCoderExecutionTransport(null);
});

it("isolates identical project IDs and paths on different workshops and runtimes", () => {
  let workshop = "workshop-a";
  setActiveWorkshopIdPort(() => workshop);
  const project = { workId: "same-id", workspaceRoot: "/repo" };
  const first = codeWorkspaceScopeKey(project);
  workshop = "workshop-b";
  expect(codeWorkspaceScopeKey(project)).not.toBe(first);
  workshop = "workshop-a";
  setCoderExecutionTransport("destination-runtime");
  expect(codeWorkspaceScopeKey(project)).not.toBe(first);
});

it("invalidates an earlier visit even when the workshop ID is unchanged", () => {
  const current = captureCodeScope(codeExecutionScopeKey);
  invalidateCodeWorkshopContext();
  expect(current()).toBe(false);
});

it("changes checkout identity on generation, branch, and baseline changes", () => {
  const environment = { worktree: "/repo", branch: "feature", generation: 1, baseline_oid: "base" };
  const project = { workId: "work", workspaceRoot: "/repo", environment };
  const first = codeWorkspaceScopeKey(project);
  for (const changed of [{ generation: 2 }, { branch: "other" }, { baseline_oid: "new-base" }]) {
    expect(codeWorkspaceScopeKey({ ...project, environment: { ...environment, ...changed } })).not.toBe(first);
  }
  expect(codeWorkspaceScopeKey({ ...project, environment: { ...environment } })).toBe(first);
});
