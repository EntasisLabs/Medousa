/** @vitest-environment happy-dom */
import { afterEach, expect, it } from "vitest";
import { loadCreativePreferences, saveCreativePreferences } from "./creativePreferences";
afterEach(() => localStorage.clear());
it("retains separate brush sizes and eraser choices, isolated by workshop", () => {
  const prefs = loadCreativePreferences("one");
  prefs.tool = "erase"; prefs.selectionMode = "lasso"; prefs.eraserMode = "stroke";
  prefs.brushKind = "pencil"; prefs.brushSizes.pencil = 12; prefs.eraserSize = 28;
  prefs.textStyle = { font_family: "serif", font_size: 45, font_weight: 700, font_style: "italic" };
  saveCreativePreferences("one", prefs);
  expect(loadCreativePreferences("one")).toEqual(prefs);
  expect(loadCreativePreferences("two")).toMatchObject({ tool: "select", selectionMode: "box", eraserMode: "partial" });
});
it("rejects corrupt or unsupported preferences without blocking drawing", () => {
  localStorage.setItem("medousa:create-tools:v1:one", JSON.stringify({ tool: "oops", brushKind: "oops", brushSizes: { pen: -1, marker: 999999 }, color: "url(bad)", eraserSize: 0 }));
  expect(loadCreativePreferences("one")).toEqual(loadCreativePreferences("empty"));
  localStorage.setItem("medousa:create-tools:v1:one", "broken json");
  expect(loadCreativePreferences("one").selectionMode).toBe("box");
});
