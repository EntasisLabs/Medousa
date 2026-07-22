import { describe, expect, it } from "vitest";
import {
  embedWidthClass,
  parseEmbedWidth,
  setEmbedWidthInBody,
} from "./liveEmbedWidth";

describe("liveEmbedWidth", () => {
  it("parses valid width and ignores invalid", () => {
    expect(parseEmbedWidth("title: A\nwidth: narrow\n")).toBe("narrow");
    expect(parseEmbedWidth("width: full")).toBe("full");
    expect(parseEmbedWidth("width: banana")).toBeNull();
    expect(parseEmbedWidth("title: only")).toBeNull();
  });

  it("round-trips setEmbedWidthInBody", () => {
    const body = "title: Deck\ncolumns: 2\n";
    const next = setEmbedWidthInBody(body, "medium");
    expect(parseEmbedWidth(next)).toBe("medium");
    expect(next).toContain("columns: 2");
    const cleared = setEmbedWidthInBody(next, null);
    expect(parseEmbedWidth(cleared)).toBeNull();
    expect(cleared).toContain("title: Deck");
  });

  it("builds CSS class for width", () => {
    expect(embedWidthClass("narrow")).toContain("vault-live-embed-width--narrow");
    expect(embedWidthClass(null)).toContain("vault-live-embed-width--wide");
  });
});
