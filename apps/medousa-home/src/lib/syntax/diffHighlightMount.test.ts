// @vitest-environment happy-dom
import { expect, it } from "vitest";
import { highlightDiffLine } from "./highlightDiffLine";
it("mounts syntax classes for a standalone diff without an editor", () => {
  const spans = highlightDiffLine("const value = 42;", "typescript");
  expect(spans.map(s => s.text).join("")).toBe("const value = 42;");
  const keyword = spans.find(s => s.text === "const");
  expect(keyword?.style).toBeTruthy();
  const styles = [...document.querySelectorAll("style")].map(s => s.textContent).join("\n");
  expect(styles).toContain(`.${keyword!.style}`);
  expect(styles).toContain("color:");
});
