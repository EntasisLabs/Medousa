import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  decodeLiquidProps,
  preprocessLiquidEmbeds,
} from "$lib/markdown/liquidEmbeds";
import { GUIDE_CHAPTERS } from "./catalog";
import {
  buildLiquidCatalogMarkdown,
  GUIDE_LIQUID_PRIMARY_LANGS,
} from "./liquidCatalogDemos";
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

  it("hydrates Liquid on front-door and FAQ chapters", () => {
    const ids = [
      "welcome",
      "find-answers",
      "architecture",
      "getting-started",
      "faq-limits",
    ] as const;
    for (const id of ids) {
      const md = loadGuideMarkdown(id);
      expect(md, id).toBeTruthy();
      const out = preprocessLiquidEmbeds(md!);
      expect(out, id).not.toMatch(/```(accordion|card|tabs|steps|callout)\b/);
      expect(out, id).toMatch(/data-liquid-embed=/);
    }

    const findAnswers = preprocessLiquidEmbeds(loadGuideMarkdown("find-answers")!);
    const idsFound: string[] = [];
    for (const m of findAnswers.matchAll(/data-liquid-props="([^"]+)"/g)) {
      const props = decodeLiquidProps<{ items?: { id: string }[] }>(m[1]);
      for (const item of props?.items ?? []) idsFound.push(item.id);
    }
    expect(idsFound).toContain("offline");
    expect(idsFound).toContain("allow-or-approve");
    expect(idsFound).toContain("phone");
  });

  it("hydrates Liquid on enriched everyday / more chapters", () => {
    const ids = [
      "work-jobs",
      "navigation-surfaces",
      "troubleshooting",
      "operator-recipes",
      "browser",
      "runtime-telemetry",
      "chat",
      "permissions-budgets",
    ] as const;
    for (const id of ids) {
      const md = loadGuideMarkdown(id);
      expect(md, id).toBeTruthy();
      const out = preprocessLiquidEmbeds(md!);
      expect(out, id).toMatch(/data-liquid-(embed|static)=/);
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

  it("keeps the generated Liquid catalog chapter in sync", () => {
    const appRoot = join(process.cwd());
    const catalogPath = join(appRoot, "src/lib/guide/pages/23-liquid-reference.md");
    const committed = readFileSync(catalogPath, "utf8");
    expect(committed).toBe(buildLiquidCatalogMarkdown());

    const preprocessed = preprocessLiquidEmbeds(committed);
    for (const lang of GUIDE_LIQUID_PRIMARY_LANGS) {
      expect(committed, `live fence ${lang}`).toContain("```" + lang + "\n");
      expect(committed, `shielded source ${lang}`).toContain(
        "````markdown\n```" + lang + "\n",
      );
      if (lang === "kanban") {
        expect(preprocessed, `live kanban`).toContain('data-liquid-static="kanban"');
      } else {
        expect(preprocessed, `live embed ${lang}`).toContain(
          `data-liquid-embed="${lang}"`,
        );
      }
    }
    // Snapshot timeline is extra (same lang as catalog timeline).
    expect(committed).toContain("layout: snapshot");
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
