import { describe, expect, it } from "vitest";
import { resolveTurnContent } from "./resolveTurnContent";

describe("resolveTurnContent", () => {
  it("prefers the terminal body when prose was streamed", () => {
    expect(resolveTurnContent("Hello world", "Final answer", true)).toBe(
      "Final answer",
    );
  });

  it("uses final_text on terminal when streamed body is empty", () => {
    expect(resolveTurnContent("", "Final answer", true)).toBe("Final answer");
  });

  it("replaces a partial streamed body with the terminal answer", () => {
    expect(resolveTurnContent("partial before tools", "Done after tools", true)).toBe(
      "Done after tools",
    );
  });

  it("returns final body for non-terminal commits", () => {
    expect(resolveTurnContent("draft", "replacement", false)).toBe(
      "replacement",
    );
  });
});
