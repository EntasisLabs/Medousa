import { describe, expect, it } from "vitest";
import {
  listFrontmatterScalarFields,
  parseFrontmatterKindValue,
  parseFrontmatterTags,
  parseFrontmatterTitle,
  removeFrontmatterFieldYaml,
  setFrontmatterFieldYaml,
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

  it("lists and upserts extra scalar fields (skips managed keys)", () => {
    let fm = "title: Hello\nkind: note\nauthor: Ada\nstatus: draft\ntags: [a]";
    expect(listFrontmatterScalarFields(fm)).toEqual([
      { key: "author", value: "Ada" },
      { key: "status", value: "draft" },
    ]);

    fm = setFrontmatterFieldYaml(fm, "status", "shipped");
    expect(listFrontmatterScalarFields(fm)).toEqual([
      { key: "author", value: "Ada" },
      { key: "status", value: "shipped" },
    ]);

    fm = setFrontmatterFieldYaml(fm, "priority", "high");
    expect(listFrontmatterScalarFields(fm).map((f) => f.key)).toEqual([
      "author",
      "status",
      "priority",
    ]);

    fm = removeFrontmatterFieldYaml(fm, "author");
    expect(listFrontmatterScalarFields(fm).map((f) => f.key)).toEqual([
      "status",
      "priority",
    ]);

    fm = setFrontmatterFieldYaml(fm, "status", "");
    expect(listFrontmatterScalarFields(fm).map((f) => f.key)).toEqual(["priority"]);
  });
});
