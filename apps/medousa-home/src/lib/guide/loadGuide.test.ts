import { describe, expect, it } from "vitest";
import { GUIDE_CHAPTERS } from "./catalog";
import { loadGuideMarkdown, missingGuidePages } from "./loadGuide";
import { parseGuideHref } from "./openGuide";

describe("guide pages", () => {
  it("bundles every catalog chapter", () => {
    expect(missingGuidePages()).toEqual([]);
  });

  it("loads markdown for each chapter id", () => {
    for (const chapter of GUIDE_CHAPTERS) {
      const md = loadGuideMarkdown(chapter.id);
      expect(md, chapter.id).toBeTruthy();
      expect(md!.length).toBeGreaterThan(40);
    }
  });
});

describe("parseGuideHref", () => {
  it("parses chapter and optional anchor", () => {
    expect(parseGuideHref("guide:chat")).toEqual({ chapterId: "chat", anchor: null });
    expect(parseGuideHref("guide:keyboard-flow#panes")).toEqual({
      chapterId: "keyboard-flow",
      anchor: "panes",
    });
  });

  it("rejects unknown chapters", () => {
    expect(parseGuideHref("guide:nope")).toBeNull();
    expect(parseGuideHref("https://example.com")).toBeNull();
  });
});
