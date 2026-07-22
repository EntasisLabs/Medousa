import { describe, expect, it } from "vitest";
import {
  folderPrefixFromNotePath,
  joinVaultFolder,
  pathForTemplate,
  resolveTemplateForSpace,
} from "./vaultTemplates";

describe("vaultTemplates location helpers", () => {
  it("derives parent folder from a note path", () => {
    expect(folderPrefixFromNotePath("projects/acme/plan.md")).toBe("projects/acme/");
    expect(folderPrefixFromNotePath("readme.md")).toBe("");
    expect(folderPrefixFromNotePath(null)).toBeNull();
  });

  it("joins optional subfolders under a prefix", () => {
    expect(joinVaultFolder("projects/acme/", "Sprint notes")).toBe(
      "projects/acme/sprint-notes/",
    );
    expect(joinVaultFolder("finance/", "")).toBe("finance/");
  });

  it("does not clamp kinds to a space", () => {
    expect(resolveTemplateForSpace("journal", "ledger")).toBe("ledger");
    expect(resolveTemplateForSpace("finance", "blank")).toBe("blank");
  });

  it("places any kind under an explicit folder prefix", () => {
    expect(
      pathForTemplate("ledger", "journal", "Q3 Burn", new Date("2026-07-21T12:00:00Z"), "projects/acme/"),
    ).toBe("projects/acme/q3-burn.md");
  });
});
