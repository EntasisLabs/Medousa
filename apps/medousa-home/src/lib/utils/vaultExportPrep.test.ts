/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import {
  expandDetailsForExport,
  hardenExportLayout,
  stripExportChrome,
} from "./vaultExportPrep";
import { buildExportPrintCss } from "./vaultExportPrintCss";
import { DEFAULT_VAULT_EXPORT_OPTIONS } from "./vaultExportOptions";

describe("vaultExportPrep helpers", () => {
  it("expands all details elements", () => {
    const root = document.createElement("div");
    root.innerHTML = `<details><summary>Q</summary><p>A</p></details>`;
    const details = root.querySelector("details")!;
    expect(details.open).toBe(false);
    expandDetailsForExport(root);
    expect(details.open).toBe(true);
    expect(details.hasAttribute("open")).toBe(true);
  });

  it("strips export chrome", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <button class="markdown-code-copy">copy</button>
      <div class="liquid-chart-toolbar">cfg</div>
      <p class="keep">hi</p>
    `;
    stripExportChrome(root);
    expect(root.querySelector(".markdown-code-copy")).toBeNull();
    expect(root.querySelector(".liquid-chart-toolbar")).toBeNull();
    expect(root.querySelector(".keep")?.textContent).toBe("hi");
  });

  it("hardens table/embed widths", () => {
    const root = document.createElement("div");
    root.innerHTML = `<table class="liquid-compare-table"></table>`;
    const table = root.querySelector("table") as HTMLElement;
    hardenExportLayout(root);
    expect(table.style.minWidth).toBe("0");
    expect(table.style.width).toBe("100%");
  });
});

describe("vaultExportPrintCss", () => {
  it("includes compare print-paper rules and optional H2 breaks", () => {
    const css = buildExportPrintCss({
      ...DEFAULT_VAULT_EXPORT_OPTIONS,
      breakBeforeH2: true,
      keepTogether: true,
      fontFamily: "serif",
      baseFontPx: 12,
    });
    expect(css).toContain(".liquid-compare");
    expect(css).toContain("liquid-compare-cell");
    expect(css).toContain("break-before: page");
    expect(css).toContain("Georgia");
    expect(css).toContain("font-size: 12px");
    expect(css).toContain('data-export-paper="1"');
  });
});
