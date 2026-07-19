import { describe, expect, it } from "vitest";
import {
  markdownToDocxChildren,
  renderVaultNoteDocxBlob,
  vaultDocxFilename,
} from "./vaultDocxExport";

describe("vaultDocxExport", () => {
  it("builds a non-empty docx blob", async () => {
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
    });
    expect(blob.size).toBeGreaterThan(500);
    expect(vaultDocxFilename("Sample Note")).toBe("sample-note.docx");
  });

  it("parses headings and tables into children", () => {
    const children = markdownToDocxChildren("# Title\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n");
    expect(children.length).toBeGreaterThanOrEqual(2);
  });
});
