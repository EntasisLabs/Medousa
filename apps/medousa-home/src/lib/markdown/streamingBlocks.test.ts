import { describe, expect, it } from "vitest";

import { StreamingMarkdownBlocks } from "$lib/markdown/streamingBlocks";

describe("StreamingMarkdownBlocks", () => {
  it("keeps the final incomplete block mutable", () => {
    const blocks = new StreamingMarkdownBlocks();
    expect(blocks.update("first", false)).toEqual({
      reset: false,
      completed: [],
      tail: "first",
    });
    expect(blocks.update("first\n\nsecond", false)).toEqual({
      reset: false,
      completed: [],
      tail: "first\n\nsecond",
    });
    expect(blocks.update("first\n\nsecond grows\n\nthird", false)).toEqual({
      reset: false,
      completed: ["first\n\n"],
      tail: "second grows\n\nthird",
    });
  });

  it("does not commit an unterminated fenced block", () => {
    const blocks = new StreamingMarkdownBlocks();
    const open = "intro\n\n```rust\nfn main() {";
    const first = blocks.update(open, false);
    expect(first.completed).toEqual([]);
    expect(first.tail).toBe(open);

    const closed = `${open}\n}\n\`\`\`\n\nafter\n\nlast`;
    const second = blocks.update(closed, false);
    expect(second.completed.join("")).toBe("intro\n\n```rust\nfn main() {\n}\n```\n\n");
    expect(second.tail).toBe("after\n\nlast");
  });

  it("finalizes only the remaining tail at terminal", () => {
    const blocks = new StreamingMarkdownBlocks();
    const source = "# Heading\n\nbody";
    const streaming = blocks.update(source, false);
    expect(streaming.completed).toEqual([]);
    expect(streaming.tail).toBe(source);
    expect(blocks.update(source, true)).toEqual({
      reset: false,
      completed: [source],
      tail: "",
    });
  });

  it("resets when canonical terminal content diverges from the stream", () => {
    const blocks = new StreamingMarkdownBlocks();
    blocks.update("draft\n\ncontinued", false);
    expect(blocks.update("canonical answer", true)).toEqual({
      reset: true,
      completed: ["canonical answer"],
      tail: "",
    });
  });

  it("holds reference syntax that later definitions can change", () => {
    const blocks = new StreamingMarkdownBlocks();
    const source = "See [the source][ref].\n\nMore prose.\n\n[ref]: https://example.invalid";
    expect(blocks.update(source, false)).toEqual({
      reset: false,
      completed: [],
      tail: source,
    });
    expect(blocks.update(source, true).completed).toEqual([source]);
  });

  it("holds shortcut references that a later definition can activate", () => {
    const blocks = new StreamingMarkdownBlocks();
    const prefix = "A [shortcut] appears here.\n\nAnother paragraph.\n\nThird paragraph.\n\n";

    expect(blocks.update(prefix, false)).toEqual({
      reset: false,
      completed: [],
      tail: prefix,
    });

    const source = `${prefix}[shortcut]: https://example.invalid\n`;
    expect(blocks.update(source, true)).toEqual({
      reset: false,
      completed: [source],
      tail: "",
    });
  });

  it("reconstructs a mixed 100k stream without losing bytes", () => {
    const unit = "## Heading\n\nparagraph\n\n- one\n- two\n\n```ts\nconst x = 1;\n```\n\n";
    const source = unit.repeat(Math.ceil(100_000 / unit.length)).slice(0, 100_000);
    const blocks = new StreamingMarkdownBlocks();
    const completed: string[] = [];
    let tail = "";
    for (let end = 37; end < source.length; end += 37) {
      const update = blocks.update(source.slice(0, end), false);
      completed.push(...update.completed);
      tail = update.tail;
    }
    const terminal = blocks.update(source, true);
    completed.push(...terminal.completed);
    tail = terminal.tail;
    expect(completed.join("") + tail).toBe(source);
  });

  it("renders the same final Markdown at different fragment cadences", async () => {
    const { marked } = await import("marked");
    const unit = [
      "## Heading\n\n",
      "paragraph with **strong** text\n\n",
      "| a | b |\n| - | - |\n| 1 | 2 |\n\n",
      "- one\n- two\n\n",
      "```ts\nconst x = 1;\n```\n\n",
    ].join("");
    const source = unit.repeat(20);
    const expected = marked.parse(source);

    for (const fragmentBytes of [8, 32, 256]) {
      const blocks = new StreamingMarkdownBlocks();
      const completed: string[] = [];
      for (let end = fragmentBytes; end < source.length; end += fragmentBytes) {
        completed.push(...blocks.update(source.slice(0, end), false).completed);
      }
      completed.push(...blocks.update(source, true).completed);
      expect(completed.map((block) => marked.parse(block)).join("")).toBe(expected);
    }
  });
});
