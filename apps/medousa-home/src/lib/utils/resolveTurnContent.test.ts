import { describe, expect, it } from "vitest";
import { resolveTurnContent } from "./resolveTurnContent";

describe("resolveTurnContent", () => {
  it("prefers streamed body on terminal when prose was streamed", () => {
    expect(resolveTurnContent("Hello world", "Final answer", true)).toBe(
      "Hello world",
    );
  });

  it("uses final_text on terminal when streamed body is empty", () => {
    expect(resolveTurnContent("", "Final answer", true)).toBe("Final answer");
  });

  it("prefers final_text after tool loop even when streamed body exists", () => {
    expect(
      resolveTurnContent("partial before tools", "Done after tools", true, {
        afterToolLoop: true,
      }),
    ).toBe("Done after tools");
  });

  it("returns final body for non-terminal commits", () => {
    expect(resolveTurnContent("draft", "replacement", false)).toBe(
      "replacement",
    );
  });
});
