import { describe, expect, it } from "vitest";
import { narrationChunks, narrationTextFromMarkdown } from "$lib/utils/narrationText";

describe("narrationTextFromMarkdown", () => {
  it("keeps human prose while removing visual markdown and raw links", () => {
    expect(
      narrationTextFromMarkdown(
        "## Why\n- Read the [guide](https://example.com/docs).\n- Use <strong>`cargo check`</strong>.\n\n```rs\nfn main() {}\n```",
      ),
    ).toBe(
      "Why Read the guide. Use cargo check. Code block available in the written response.",
    );
  });

  it("returns an empty string for visual-only whitespace", () => {
    expect(narrationTextFromMarkdown("  \n\n  ")).toBe("");
  });
});

describe("narrationChunks", () => {
  it("preserves all text in bounded utterances", () => {
    const source =
      "The first relationship is causal. The second sentence explains the boundary. " +
      "A final example transfers the concept somewhere new.";
    const chunks = narrationChunks(source, 80);
    expect(chunks.length).toBeGreaterThan(1);
    expect(chunks.every((chunk) => chunk.length <= 80)).toBe(true);
    expect(chunks.join(" ")).toBe(source);
  });
});
