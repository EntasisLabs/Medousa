import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
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

  it("keeps the generated commands appendix in sync", () => {
    const appRoot = join(process.cwd());
    const appendixPath = join(appRoot, "src/lib/guide/pages/24-commands-reference.md");
    const committed = readFileSync(appendixPath, "utf8");
    const tmp = join(mkdtempSync(join(tmpdir(), "medousa-guide-")), "out.md");
    execFileSync(process.execPath, ["scripts/generate-guide-appendix.mjs", tmp], {
      cwd: appRoot,
      stdio: "pipe",
    });
    const fresh = readFileSync(tmp, "utf8");
    expect(committed).toBe(fresh);
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
