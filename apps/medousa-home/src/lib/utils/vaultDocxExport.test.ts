/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import {
  htmlExportToDocxChildren,
  markdownToDocxChildren,
  renderVaultNoteDocxBlob,
  vaultDocxFilename,
} from "./vaultDocxExport";
import { DEFAULT_VAULT_EXPORT_OPTIONS } from "./vaultExportOptions";

describe("vaultDocxExport", () => {
  it("builds a non-empty docx blob from markdown", async () => {
    const blob = await renderVaultNoteDocxBlob({
      title: "Sample",
      content: `# Hello

Paragraph with **bold**.

| a | b |
| --- | --- |
| 1 | 2 |

\`\`\`ts
const x = 1;
\`\`\`
`,
      labelByPath: new Map(),
    });
    expect(blob.size).toBeGreaterThan(500);
    expect(vaultDocxFilename("Sample Note")).toBe("sample-note.docx");
  });

  it("maps fixture HTML headings, tables, tasks, and page breaks", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <h1>Title</h1>
      <p>Hello <strong>world</strong></p>
      <ul>
        <li><input type="checkbox" checked disabled /> Done</li>
        <li><input type="checkbox" disabled /> Todo</li>
      </ul>
      <div class="vault-export-page-break"></div>
      <table>
        <tr><th>a</th><th>b</th></tr>
        <tr><td>1</td><td>2</td></tr>
      </table>
      <pre><code>const x = 1;</code></pre>
    `;
    const children = htmlExportToDocxChildren(
      root,
      DEFAULT_VAULT_EXPORT_OPTIONS,
    );
    expect(children.length).toBeGreaterThanOrEqual(5);
  });

  it("parses headings and tables via markdownToDocxChildren helper", () => {
    const children = markdownToDocxChildren(
      "# Title\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n",
    );
    expect(children.length).toBeGreaterThanOrEqual(2);
  });
});
