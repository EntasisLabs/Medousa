/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it } from "vitest";
import {
  DEFAULT_VAULT_EXPORT_OPTIONS,
  exportContentWidthPx,
  exportFontStack,
  exportMarginInches,
  normalizeVaultExportOptions,
  readVaultExportOptions,
  slugifyExportFilename,
  vaultExportFilename,
  writeVaultExportOptions,
} from "./vaultExportOptions";

describe("vaultExportOptions", () => {
  afterEach(() => {
    if (typeof localStorage !== "undefined") {
      localStorage.removeItem("medousa-vault-export-options");
    }
  });

  it("normalizes invalid values to defaults", () => {
    const opts = normalizeVaultExportOptions({
      fontFamily: "comic" as never,
      baseFontPx: 99,
      pageSize: "tabloid" as never,
      margins: "huge" as never,
    });
    expect(opts.fontFamily).toBe("system");
    expect(opts.baseFontPx).toBe(16);
    expect(opts.pageSize).toBe("letter");
    expect(opts.margins).toBe("comfortable");
    expect(opts.keepTogether).toBe(false);
  });

  it("defaults keepTogether to false and honors explicit true", () => {
    expect(DEFAULT_VAULT_EXPORT_OPTIONS.keepTogether).toBe(false);
    expect(normalizeVaultExportOptions(null).keepTogether).toBe(false);
    expect(
      normalizeVaultExportOptions({ keepTogether: true }).keepTogether,
    ).toBe(true);
  });

  it("persists and reads options", () => {
    writeVaultExportOptions({
      ...DEFAULT_VAULT_EXPORT_OPTIONS,
      fontFamily: "serif",
      pageSize: "a4",
      breakBeforeH2: true,
    });
    const read = readVaultExportOptions();
    expect(read.fontFamily).toBe("serif");
    expect(read.pageSize).toBe("a4");
    expect(read.breakBeforeH2).toBe(true);
  });

  it("builds filename and font stacks", () => {
    expect(vaultExportFilename("Hello World", "pdf")).toBe("hello-world.pdf");
    expect(vaultExportFilename("Hello World", "docx")).toBe("hello-world.docx");
    expect(slugifyExportFilename("!!!")).toBe("note");
    expect(exportFontStack("serif")).toContain("Georgia");
    expect(exportMarginInches("compact")[0]).toBeLessThan(
      exportMarginInches("wide")[0],
    );
    expect(exportContentWidthPx(DEFAULT_VAULT_EXPORT_OPTIONS)).toBeGreaterThan(
      400,
    );
  });
});
