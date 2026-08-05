import { describe, expect, it } from "vitest";
import { renderWebviewMarkdown } from "./liquidWebview.js";

describe("VS Code Liquid Markdown adapter", () => {
  it("turns Liquid fences into inert placeholders instead of code blocks", () => {
    const html = renderWebviewMarkdown([
      "Before",
      "```card",
      "title: Shared",
      "body: Portable",
      "```",
      "After",
    ].join("\n"));

    expect(html).toContain('data-liquid-embed="card"');
    expect(html).not.toContain("data-copy-code");
    expect(html).toContain("<p>Before</p>");
    expect(html).toContain("<p>After</p>");
  });

  it("keeps ordinary code fences and strips Home-only chart controls", () => {
    const code = renderWebviewMarkdown("```ts\nconst answer = 42;\n```");
    expect(code).toContain("data-copy-code");
    expect(code).toContain("const answer = 42;");

    const chart = renderWebviewMarkdown([
      "```chart",
      "type: bar",
      "| Month | Visits |",
      "| ----- | ------ |",
      "| Jan | 12 |",
      "| Feb | 18 |",
      "```",
    ].join("\n"));
    expect(chart).toContain('data-liquid-embed="chart"');
    expect(chart).not.toContain("liquid-chart-configure");
  });

  it("preserves generated static kanban markup but escapes authored HTML", () => {
    const kanban = renderWebviewMarkdown([
      "```kanban",
      "## Doing",
      "- [ ] Ship extensions",
      "```",
    ].join("\n"));
    expect(kanban).toContain('data-liquid-static="kanban"');

    const malicious = renderWebviewMarkdown('<div class="liquid-mini-kanban"><img src=x onerror=boom></div>');
    expect(malicious).toContain("&lt;div");
    expect(malicious).not.toContain("<img");
  });

  it("escapes prose while preserving allowlisted icon shortcodes", () => {
    const html = renderWebviewMarkdown('<script>alert(1)</script> {{icon:sparkles}}');
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).toContain('data-liquid-icon="sparkles"');
  });
});
