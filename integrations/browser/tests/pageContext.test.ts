import { describe, expect, it } from "vitest";
import { boundPageText } from "../src/pageContext.js";

describe("browser page context", () => {
  it("bounds captured page text at the shared context limit", () => {
    expect(boundPageText(`${"x".repeat(30_000)}   `)).toHaveLength(24_000);
  });

  it("removes trailing whitespace without changing readable content", () => {
    expect(boundPageText("Heading\n\nBody   ")).toBe("Heading\n\nBody");
  });
});
