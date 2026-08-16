/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it } from "vitest";
import {
  applySelectedThemeStylesheet,
  selectedThemeHref,
} from "./themeStylesheet";

afterEach(() => {
  document.getElementById("medousa-selected-theme")?.remove();
});

describe("selected theme stylesheet", () => {
  it("points at the catalog token path", () => {
    expect(selectedThemeHref("black-lily")).toBe("/themes/black-lily.css");
  });

  it("replaces the live stylesheet when the palette changes", () => {
    applySelectedThemeStylesheet("medousa");
    applySelectedThemeStylesheet("black-lily");
    const link = document.getElementById("medousa-selected-theme") as HTMLLinkElement;
    expect(link.rel).toBe("stylesheet");
    expect(link.getAttribute("href")).toBe("/themes/black-lily.css");
    expect(document.querySelectorAll("#medousa-selected-theme")).toHaveLength(1);
  });
});
