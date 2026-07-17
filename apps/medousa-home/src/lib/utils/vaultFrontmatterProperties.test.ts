import { describe, expect, it } from "vitest";
import {
  parseFrontmatterKindValue,
  parseFrontmatterTags,
  parseFrontmatterTitle,
  setFrontmatterKindYaml,
  setFrontmatterTagsYaml,
  setFrontmatterTitleYaml,
} from "./vaultFrontmatter";

describe("Live frontmatter property helpers", () => {
  it("reads and writes title / kind / tags", () => {
    let fm = "kind: research\ntags: [ai, models]";
    expect(parseFrontmatterKindValue(fm)).toBe("research");
    expect(parseFrontmatterTags(fm)).toEqual(["ai", "models"]);

    fm = setFrontmatterTitleYaml(fm, "Frontier models · mid-2026");
    expect(parseFrontmatterTitle(fm)).toBe("Frontier models · mid-2026");

    fm = setFrontmatterKindYaml(fm, "note");
    expect(parseFrontmatterKindValue(fm)).toBe("note");

    fm = setFrontmatterTagsYaml(fm, ["ai", "liquid-demo"]);
    expect(parseFrontmatterTags(fm)).toEqual(["ai", "liquid-demo"]);
  });

  it("creates frontmatter when missing", () => {
    const fm = setFrontmatterTagsYaml(null, ["vault"]);
    expect(parseFrontmatterTags(fm)).toEqual(["vault"]);
  });
});
