/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import {
  applyPaperColorsAfterSanitize,
  bodyHasMatchingTitleH1,
  densifyCompareForExport,
  ensureTableHeadersForExport,
  expandDetailsForExport,
  glueHeadingsToFollowingEmbed,
  glueLabelParagraphsToFollowing,
  hardenExportLayout,
  isLabelLikeParagraph,
  markTallEmbedsForPageFlow,
  normalizeExportTitle,
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

  it("dedupes title when body already starts with matching h1", () => {
    expect(normalizeExportTitle("  Frontier Models  ")).toBe("frontier models");
    const body = document.createElement("div");
    body.innerHTML = `<h1>Frontier Models</h1><p>Intro</p>`;
    expect(bodyHasMatchingTitleH1(body, "frontier models")).toBe(true);
    expect(bodyHasMatchingTitleH1(body, "Other")).toBe(false);
    const noH1 = document.createElement("div");
    noH1.innerHTML = `<h2>Section</h2>`;
    expect(bodyHasMatchingTitleH1(noH1, "Section")).toBe(false);
  });

  it("keeps compare/section units unless taller than a page", () => {
    const root = document.createElement("div");
    const section = document.createElement("div");
    section.className = "vault-export-section";
    Object.defineProperty(section, "scrollHeight", { value: 520 });
    root.appendChild(section);
    const tooTall = document.createElement("div");
    tooTall.className = "liquid-compare";
    Object.defineProperty(tooTall, "scrollHeight", { value: 980 });
    root.appendChild(tooTall);
    const brief = document.createElement("div");
    brief.className = "liquid-brief";
    Object.defineProperty(brief, "scrollHeight", { value: 280 });
    root.appendChild(brief);
    markTallEmbedsForPageFlow(root, 1000, true);
    expect(section.classList.contains("vault-export-keep")).toBe(true);
    expect(tooTall.classList.contains("vault-export-allow-break")).toBe(true);
    expect(brief.classList.contains("vault-export-keep")).toBe(true);
  });

  it("densifies compare tables so columns are not clipped", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <div class="liquid-compare-scroll">
        <table class="liquid-compare-table">
          <tr><th class="liquid-compare-entity">A</th></tr>
        </table>
      </div>
    `;
    densifyCompareForExport(root);
    const table = root.querySelector(".liquid-compare-table") as HTMLElement;
    const scroll = root.querySelector(".liquid-compare-scroll") as HTMLElement;
    const entity = root.querySelector(".liquid-compare-entity") as HTMLElement;
    expect(table.style.width).toBe("100%");
    expect(table.style.tableLayout).toBe("fixed");
    expect(scroll.style.overflowX).toBe("visible");
    expect(entity.style.minWidth).toBe("0");
  });

  it("glues h2 to the following liquid embed", () => {
    const body = document.createElement("div");
    body.innerHTML = `
      <h2>Compare</h2>
      <div class="liquid-md-embed" data-liquid-embed="compare">matrix</div>
      <p>after</p>
    `;
    glueHeadingsToFollowingEmbed(body);
    const section = body.querySelector(".vault-export-section");
    expect(section).toBeTruthy();
    expect(section?.querySelector("h2")?.textContent).toBe("Compare");
    expect(section?.querySelector(".liquid-md-embed")).toBeTruthy();
    expect(body.querySelector(":scope > h2")).toBeNull();
  });

  it("re-applies paper colors on callout/accordion/card after sanitize", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <div class="liquid-callout" style="color: rgb(250,250,250); background: rgb(10,10,10)"></div>
      <div class="liquid-accordion"><div class="liquid-accordion-title">Q</div></div>
      <div class="liquid-card"><div class="liquid-card-title">Card</div></div>
    `;
    applyPaperColorsAfterSanitize(root);
    const callout = root.querySelector(".liquid-callout") as HTMLElement;
    expect(callout.style.background).toContain("#f9fafb");
    expect(callout.style.color).toContain("#111827");
  });

  it("bakes paper ink onto body list items and italics", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <div class="vault-pdf-export-body">
        <ul><li style="color: rgb(200,200,200)">Ghost bullet</li></ul>
        <p><em style="color: rgb(180,180,180)">Ghost italic</em></p>
      </div>
    `;
    applyPaperColorsAfterSanitize(root);
    const li = root.querySelector("li") as HTMLElement;
    const em = root.querySelector("em") as HTMLElement;
    expect(li.style.color).toContain("#111827");
    expect(em.style.color).toContain("#111827");
  });

  it("glues bold label paragraphs to following lists", () => {
    const body = document.createElement("div");
    body.innerHTML = `
      <p><strong>Anchors:</strong></p>
      <ul><li>Toggle bolts</li></ul>
      <p>Normal paragraph.</p>
    `;
    const label = body.querySelector("p") as HTMLElement;
    expect(isLabelLikeParagraph(label)).toBe(true);
    glueLabelParagraphsToFollowing(body);
    const group = body.querySelector(".vault-export-label-group");
    expect(group).toBeTruthy();
    expect(group?.querySelector("strong")?.textContent).toBe("Anchors:");
    expect(group?.querySelector("ul")).toBeTruthy();
  });

  it("promotes first th-row into thead for header repeat", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <table>
        <tr><th>A</th><th>B</th></tr>
        <tr><td>1</td><td>2</td></tr>
      </table>
    `;
    ensureTableHeadersForExport(root);
    expect(root.querySelector("thead tr th")?.textContent).toBe("A");
    expect(root.querySelectorAll("tbody tr, table > tr").length).toBeGreaterThan(0);
  });
});

describe("vaultExportPrintCss", () => {
  it("includes callout/accordion/card packs and stronger markdown-callout", () => {
    const css = buildExportPrintCss({
      ...DEFAULT_VAULT_EXPORT_OPTIONS,
      breakBeforeH2: true,
      keepTogether: true,
      fontFamily: "serif",
      baseFontPx: 12,
    });
    expect(css).toContain(".liquid-compare");
    expect(css).toContain("liquid-compare-cell");
    expect(css).toContain(".liquid-callout");
    expect(css).toContain(".liquid-accordion");
    expect(css).toContain(".liquid-card");
    expect(css).toContain(".markdown-callout");
    expect(css).toContain("break-before: page");
    expect(css).toContain("Georgia");
    expect(css).toContain("font-size: 12px");
    expect(css).toContain('data-export-paper="1"');
  });

  it("keepTogether avoids small units only — not blanket table or liquid-md-embed", () => {
    const css = buildExportPrintCss({
      ...DEFAULT_VAULT_EXPORT_OPTIONS,
      keepTogether: true,
    });
    expect(css).toContain(".liquid-callout");
    expect(css).toContain(".liquid-compare-card");
    expect(css).toContain("break-inside: avoid");
    // Must not apply avoid to the entire embed host
    expect(css).not.toMatch(
      /\.liquid-md-embed\s*\{[^}]*break-inside:\s*avoid/s,
    );
    // Whole tables must be allowed to span pages
    expect(css).not.toMatch(
      /^\s*\.vault-pdf-export-mount table,\s*$/m,
    );
    // Rows must not split mid-cell
    expect(css).toMatch(/\.vault-pdf-export-mount tr\s*\{[^}]*break-inside:\s*avoid/s);
    expect(css).toContain("table-header-group");
  });

  it("forces paper ink on descendant li and em", () => {
    const css = buildExportPrintCss(DEFAULT_VAULT_EXPORT_OPTIONS);
    expect(css).toContain(".vault-pdf-export-body li");
    expect(css).toContain(".vault-pdf-export-body em");
    expect(css).toContain(".markdown-content li");
    expect(css).not.toContain(".vault-pdf-export-body > li");
  });

  it("keeps thead with first body row and flushes table margins", () => {
    const css = buildExportPrintCss(DEFAULT_VAULT_EXPORT_OPTIONS);
    expect(css).toMatch(
      /\.vault-pdf-export-mount thead\s*\{[^}]*break-after:\s*avoid/s,
    );
    expect(css).toContain("tbody tr:first-child");
    expect(css).toContain(".markdown-table-scroll");
    expect(css).toMatch(/margin-left:\s*0\s*!important/);
  });

  it("omits keepTogether small-unit avoid list when disabled but keeps page-flow", () => {
    const css = buildExportPrintCss({
      ...DEFAULT_VAULT_EXPORT_OPTIONS,
      keepTogether: false,
    });
    expect(css).toContain("break-inside: auto");
    expect(css).toContain(".liquid-carousel");
    expect(css).toContain("flex-wrap: wrap");
    expect(css).toContain("vault-export-keep");
    // keepTogether-only selectors should not add faceoff avoid when off
    expect(css).not.toMatch(
      /\.liquid-compare-faceoff\s*,\s*\n\s*\.vault-pdf-export-mount \.liquid-carousel-item/,
    );
  });

  it("wraps carousels and hides tab strips for export", () => {
    const css = buildExportPrintCss(DEFAULT_VAULT_EXPORT_OPTIONS);
    expect(css).toContain(".liquid-carousel");
    expect(css).toContain("flex-wrap: wrap");
    expect(css).toContain(".liquid-tabs-list");
    expect(css).toContain("display: none");
    expect(css).toContain("break-after: avoid");
    expect(css).toContain(".liquid-brief");
    expect(css).toContain("vault-export-keep");
  });
});
