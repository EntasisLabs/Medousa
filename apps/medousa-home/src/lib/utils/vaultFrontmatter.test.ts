import { describe, expect, it } from "vitest";
import {
  serializeFrontmatter,
  setFrontmatterKind,
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
});
