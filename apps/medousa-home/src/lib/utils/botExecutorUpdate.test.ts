import { describe, expect, it } from "vitest";
import type { ExternalAgentExecutor } from "$lib/types/generated/daemon_api";
import { botExecutorUpdate } from "./botExecutorUpdate";

const executor: ExternalAgentExecutor = {
  runtime: "cursor", home_workshop_id: "mini", forge_work_id: "work-app",
  forge_repo_id: "repo-app", session_contract: "fresh_per_job",
  allowed_tools: [], allowed_capabilities: [],
};
describe("Bot identity edits preserve execution permissions", () => {
  it("omits executor administration fields for native Bots", () => {
    expect(botExecutorUpdate(undefined, null)).toEqual({});
  });
  it("omits an unchanged runtime binding despite object field order", () => {
    const reordered = Object.fromEntries(Object.entries(executor).reverse()) as ExternalAgentExecutor;
    expect(botExecutorUpdate(executor, reordered)).toEqual({});
  });
  it("explicitly clears a removed binding", () => {
    expect(botExecutorUpdate(executor, null)).toEqual({ clear_external_agent: true });
  });
  it("sends changed runtime and project pins for administrative admission", () => {
    const next = { ...executor, runtime: "hermes" as const, forge_work_id: "work-new" };
    expect(botExecutorUpdate(executor, next)).toEqual({ external_agent: next });
  });
});
