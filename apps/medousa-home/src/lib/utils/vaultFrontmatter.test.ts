import { describe, expect, it } from "vitest";
import {
  parseFrontmatterAuthor,
  parseFrontmatterDate,
  restingVaultTagChips,
  serializeFrontmatter,
  setFrontmatterAuthorYaml,
  setFrontmatterDateYaml,
  setFrontmatterKind,
  sortVaultTagsForDisplay,
  stripFrontmatter,
} from "./vaultFrontmatter";

describe("vaultFrontmatter", () => {
  it("strips leading newlines inside frontmatter so rewrite stays stable", () => {
    const bloated = "---\n\n\nkind: note\n---\n\n# Hello";
    const { content, frontmatter } = stripFrontmatter(bloated);
    expect(frontmatter).toBe("kind: note");
    expect(content).toBe("# Hello");
  });

  it("does not grow blank lines across setFrontmatterKind cycles", () => {
    let body = "---\nkind: note\n---\n\n# Hello";
    for (let i = 0; i < 8; i++) {
      body = setFrontmatterKind(body, "project");
    }
    expect(body.startsWith("---\nkind: project\n---\n\n")).toBe(true);
    expect(body).not.toMatch(/^---\n\n/);
    expect(body).not.toContain("---\n\nkind");
    expect(body.match(/---/g)?.length).toBe(2);
  });

  it("serializeFrontmatter trims yaml blanks", () => {
    expect(serializeFrontmatter("\n\nkind: note\n\n", "# Hi")).toBe(
      "---\nkind: note\n---\n\n# Hi",
    );
  });

  it("sorts human tags before workshop tags", () => {
    expect(
      sortVaultTagsForDisplay([
        "medousa",
        "bonsai",
        "chat:foo",
        "phoenix",
        "vault",
      ]),
    ).toEqual(["bonsai", "phoenix", "medousa", "chat:foo", "vault"]);
  });

  it("rests at most two human tags with +N for the rest", () => {
    expect(
      restingVaultTagChips(
        ["medousa", "arizona", "bonsai", "guide", "vault"],
        2,
      ),
    ).toEqual({ visible: ["arizona", "bonsai"], hiddenCount: 3 });
  });

  it("round-trips author and date frontmatter fields", () => {
    let fm = setFrontmatterAuthorYaml(null, "Ada Lovelace");
    fm = setFrontmatterDateYaml(fm, "2026-07-18");
    expect(parseFrontmatterAuthor(fm)).toBe("Ada Lovelace");
    expect(parseFrontmatterDate(fm)).toBe("2026-07-18");
    fm = setFrontmatterAuthorYaml(fm, "");
    expect(parseFrontmatterAuthor(fm)).toBe("");
    expect(parseFrontmatterDate(fm)).toBe("2026-07-18");
  });

  it("falls back to updated when date is absent", () => {
    expect(parseFrontmatterDate("updated: 2026-01-01")).toBe("2026-01-01");
  });
});
