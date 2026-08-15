import { describe, expect, it } from "vitest";

import {
  createMarkdownRenderSession,
  renderMarkdown,
} from "$lib/markdown/render";
import { StreamingMarkdownBlocks } from "$lib/markdown/streamingBlocks";

describe("MarkdownRenderSession", () => {
  it("carries stable heading identity without tail renders mutating it", () => {
    const renderer = createMarkdownRenderSession();
    expect(renderer.renderStable("# Repeat")).toContain('id="repeat"');
    expect(renderer.renderTail("# Repeat")).toContain('id="repeat-1"');
    expect(renderer.renderTail("# Repeat grows")).toContain('id="repeat-grows"');
    expect(renderer.renderStable("# Repeat")).toContain('id="repeat-1"');
    expect(renderer.renderStable("# Repeat")).toContain('id="repeat-2"');
  });

  it("keeps interactive task indexes unique across completed blocks", () => {
    const renderer = createMarkdownRenderSession({ interactiveTasks: true });
    expect(renderer.renderStable("- [ ] first")).toContain('data-vault-task="0"');
    expect(renderer.renderTail("- [ ] pending")).toContain('data-vault-task="1"');
    expect(renderer.renderStable("- [ ] second")).toContain('data-vault-task="1"');
  });

  it("matches the terminal rich render at different fragment cadences", () => {
    const unit = [
      "## Streaming benchmark\n\n",
      "Prose with a [link](https://example.invalid) and `inline code`.\n\n",
      "| column | value |\n| --- | ---: |\n| latency | 42 |\n\n",
      "```rust\nfn main() { println!(\"bounded\"); }\n```\n\n",
      "```mermaid\ngraph TD; provider-->pipeline-->home;\n```\n\n",
      "```card\ntitle: Result\nbody: Stable\n```\n\n",
    ].join("");
    const source = unit.repeat(10);
    const expected = renderMarkdown(source);

    for (const fragmentBytes of [8, 32, 256]) {
      const blocks = new StreamingMarkdownBlocks();
      const renderer = createMarkdownRenderSession();
      let html = "";
      for (let end = fragmentBytes; end < source.length; end += fragmentBytes) {
        const update = blocks.update(source.slice(0, end), false);
        html += update.completed.map((block) => renderer.renderStable(block)).join("");
      }
      const terminal = blocks.update(source, true);
      html += terminal.completed.map((block) => renderer.renderStable(block)).join("");
      expect(html).toBe(expected);
    }
  });
});
