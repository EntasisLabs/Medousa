/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it } from "vitest";
import { activeAgent } from "$lib/stores/activeAgent.svelte";
import { applyActiveAgentPrompt } from "./activeAgentPrompt";

describe("applyActiveAgentPrompt", () => {
  beforeEach(() => {
    activeAgent.clear();
  });

  it("leaves prompt alone without agent", () => {
    expect(applyActiveAgentPrompt("hello")).toBe("hello");
  });

  it("prefixes skill when agent selected", () => {
    activeAgent.setActive("morning-brief");
    expect(applyActiveAgentPrompt("hello")).toBe("/skill morning-brief\nhello");
  });
});
