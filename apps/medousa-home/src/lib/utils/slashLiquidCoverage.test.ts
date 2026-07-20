import { describe, expect, it } from "vitest";
import { preprocessLiquidEmbeds } from "$lib/markdown/liquidEmbeds";
import { insertSlashBlock, SLASH_BLOCK_IDS, type SlashBlockId } from "./vaultMarkdownEdit";

const LIQUID_SLASH: SlashBlockId[] = SLASH_BLOCK_IDS.filter((id) =>
  id.startsWith("liquid_"),
);

describe("slash liquid coverage", () => {
  it("lists every hydrated liquid lang", () => {
    const langs = LIQUID_SLASH.map((id) => id.replace("liquid_", ""));
    for (const lang of [
      "callout",
      "card",
      "carousel",
      "actions",
      "section",
      "chips",
      "media",
      "cite",
      "compare",
      "plan",
      "timeline",
      "shortlist",
      "decision",
      "brief",
      "chart",
      "dashboard",
      "report",
      "slides",
      "tabs",
      "steps",
      "accordion",
      "code",
      "tree",
      "kanban",
    ]) {
      expect(langs).toContain(lang);
    }
  });

  it("inserts templates that hydrate", () => {
    for (const id of LIQUID_SLASH) {
      const { content } = insertSlashBlock("", 0, id);
      expect(content.trim().length).toBeGreaterThan(0);
      if (id === "liquid_chart") {
        expect(content).toContain("```chart");
        continue;
      }
      const lang = id.replace("liquid_", "");
      const out = preprocessLiquidEmbeds(content);
      if (lang === "kanban") {
        expect(out, id).toMatch(/data-liquid-(embed|static)="kanban"/);
        continue;
      }
      expect(out, id).toContain(`data-liquid-embed="${lang}"`);
    }
  });
});
