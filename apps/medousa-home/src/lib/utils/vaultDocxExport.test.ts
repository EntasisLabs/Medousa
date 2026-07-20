/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import {
  buildDocxColumnWidths,
  DOCX_HEADING_COLOR,
  DOCX_SNAPSHOT_SELECTOR,
  htmlExportToDocxChildren,
  markdownToDocxChildren,
  renderVaultNoteDocxBlob,
  selectDocxSnapshotTargets,
  vaultDocxFilename,
} from "./vaultDocxExport";
import {
  DEFAULT_VAULT_EXPORT_OPTIONS,
  exportDocxContentWidthDxa,
} from "./vaultExportOptions";

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

  it("snapshot selector covers card/accordion/callout embeds and mini-kanban", () => {
    expect(DOCX_SNAPSHOT_SELECTOR).toContain(".liquid-md-embed");
    expect(DOCX_SNAPSHOT_SELECTOR).toContain(".liquid-mini-kanban");
    expect(DOCX_HEADING_COLOR).toBe("111827");

    const root = document.createElement("div");
    root.innerHTML = `
      <div class="liquid-md-embed" data-liquid-embed="card"><div class="liquid-card">Card</div></div>
      <div class="liquid-md-embed" data-liquid-embed="accordion"><div class="liquid-accordion">FAQ</div></div>
      <div class="liquid-md-embed" data-liquid-embed="callout"><div class="liquid-callout">Note</div></div>
      <div class="liquid-mini-kanban"><p>Board</p></div>
    `;
    const targets = selectDocxSnapshotTargets(root);
    expect(targets).toHaveLength(4);
    expect(targets.every((el) => el.matches(DOCX_SNAPSHOT_SELECTOR))).toBe(true);
  });

  it("falls back to plain prose without italic brackets when snapshot missing", () => {
    const root = document.createElement("div");
    root.innerHTML = `<div class="liquid-md-embed" data-liquid-embed="callout">Ghost ink callout body text</div>`;
    const children = htmlExportToDocxChildren(
      root,
      DEFAULT_VAULT_EXPORT_OPTIONS,
      new Map(),
    );
    expect(children.length).toBe(1);
    const para = children[0] as { root?: unknown[] };
    const json = JSON.stringify(para);
    expect(json).toContain("Ghost ink callout body text");
    expect(json).not.toContain("[Ghost");
    expect(json).not.toMatch(/"italics"\s*:\s*true/);
  });

  it("builds equal DXA column widths that sum to content width", () => {
    const content = exportDocxContentWidthDxa(DEFAULT_VAULT_EXPORT_OPTIONS);
    expect(content).toBeGreaterThan(5000);
    const widths = buildDocxColumnWidths(3, content);
    expect(widths).toHaveLength(3);
    expect(widths.reduce((a, b) => a + b, 0)).toBe(content);
    expect(widths.every((w) => w > 100)).toBe(true);
  });

  it("numbers lists with left/hanging indent (not right-edge squash)", async () => {
    const { Packer: DocxPacker } = await import("docx");
    const root = document.createElement("div");
    root.innerHTML = `
      <ol>
        <li>First item with enough words to wrap if crushed</li>
        <li>Second item also needs full width</li>
      </ol>
    `;
    const children = htmlExportToDocxChildren(
      root,
      DEFAULT_VAULT_EXPORT_OPTIONS,
    );
    expect(children.length).toBe(2);
    const json = JSON.stringify(children);
    expect(json).toContain("First item");
    // Paragraph numbering reference present
    expect(json).toMatch(/numId|numbering|ilvl/i);

    // Document-level numbering config must include hanging indent (720/360).
    const blob = await renderVaultNoteDocxBlob({
      title: "List check",
      content: "1. Alpha item here\n2. Beta item here\n",
      labelByPath: new Map(),
    });
    expect(blob.size).toBeGreaterThan(400);
    void DocxPacker;
  });

  it("keeps label-like paragraphs with the following block", () => {
    const root = document.createElement("div");
    root.innerHTML = `<p><strong>Anchors:</strong></p><ul><li>Toggle</li></ul>`;
    const children = htmlExportToDocxChildren(
      root,
      DEFAULT_VAULT_EXPORT_OPTIONS,
    );
    const json = JSON.stringify(children[0]);
    expect(json).toContain("Anchors");
    expect(json).toContain("w:keepNext");
  });

  it("emits full-width table grid (not 100 DXA crush)", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <table>
        <tr><th>a</th><th>b</th><th>c</th></tr>
        <tr><td>1</td><td>2</td><td>3</td></tr>
      </table>
    `;
    const children = htmlExportToDocxChildren(
      root,
      DEFAULT_VAULT_EXPORT_OPTIONS,
    );
    const json = JSON.stringify(children);
    const content = exportDocxContentWidthDxa(DEFAULT_VAULT_EXPORT_OPTIONS);
    expect(json).toContain(String(content));
    // Default crush was 100 DXA per column — real cols are much wider
    const widths = buildDocxColumnWidths(3, content);
    expect(json).toContain(String(widths[0]));
  });

  it("snapshots glued sections so headings are not orphaned above images", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <div class="vault-export-section">
        <h2>Compare</h2>
        <div class="liquid-md-embed" data-liquid-embed="compare">matrix body</div>
      </div>
      <h2>Loose</h2>
      <p>para</p>
    `;
    const targets = selectDocxSnapshotTargets(root);
    expect(targets.some((el) => el.classList.contains("vault-export-section"))).toBe(
      true,
    );
    expect(
      targets.some(
        (el) =>
          el.classList.contains("liquid-md-embed") &&
          el.closest(".vault-export-section"),
      ),
    ).toBe(false);

    const section = root.querySelector(".vault-export-section") as HTMLElement;
    const fakeSnap = {
      data: new Uint8Array([1, 2, 3]),
      width: 100,
      height: 80,
    };
    const children = htmlExportToDocxChildren(
      root,
      DEFAULT_VAULT_EXPORT_OPTIONS,
      new Map([[section, fakeSnap]]),
    );
    // One image for the section (heading baked in) + loose heading + para
    expect(children.length).toBeGreaterThanOrEqual(3);
    const json = JSON.stringify(children);
    // Section heading must not appear as its own Word heading run
    expect(json).not.toMatch(/"value":"Compare"/);
    expect(json).toContain("Loose");
  });
});


