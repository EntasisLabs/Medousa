import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { allThemes } from "./theme-catalog";
import {
  themeCatalogEntries,
  themePropertiesToCss,
  writeThemeCssFiles,
} from "./theme-css";

describe("theme token catalog", () => {
  it("exports names and token paths without compiling every palette", () => {
    const catalog = themeCatalogEntries(allThemes);
    expect(catalog.length).toBe(allThemes.length);
    expect(catalog.every((entry) => entry.tokenPath === `/themes/${entry.name}.css`)).toBe(
      true,
    );
    expect(catalog.some((entry) => entry.name === "medousa")).toBe(true);
  });

  it("emits a selected-theme variable sheet", () => {
    const css = themePropertiesToCss(allThemes[0]!);
    expect(css).toContain('[data-theme="medousa"]');
    expect(css).toContain("--theme-canvas:");
    expect(css).not.toContain("[data-theme=\"black-lily\"]");
  });

  it("writes one css file per catalog theme", () => {
    const dir = mkdtempSync(join(tmpdir(), "medousa-themes-"));
    const written = writeThemeCssFiles(dir, allThemes.slice(0, 2));
    expect(written).toHaveLength(2);
    expect(readFileSync(written[0]!, "utf8")).toContain("[data-theme=");
  });
});
