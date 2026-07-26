import { describe, expect, it } from "vitest";
import {
  buildComposerSlashItems,
  composerSlashToken,
  stripComposerSlashToken,
} from "./composerSkillSlash";

describe("composerSlashToken", () => {
  it("detects a trailing slash token", () => {
    expect(composerSlashToken("/hel", 4)).toEqual({
      start: 0,
      end: 4,
      filter: "hel",
      raw: "/hel",
    });
    expect(composerSlashToken("please /doc", 11)).toEqual({
      start: 7,
      end: 11,
      filter: "doc",
      raw: "/doc",
    });
  });

  it("ignores slash mid-word", () => {
    expect(composerSlashToken("a/b", 3)).toBeNull();
  });
});

describe("stripComposerSlashToken", () => {
  it("removes the token or replaces with insert", () => {
    const token = composerSlashToken("hi /x", 5)!;
    expect(stripComposerSlashToken("hi /x", token, "")).toEqual({
      value: "hi ",
      cursor: 3,
    });
    expect(stripComposerSlashToken("hi /x", token, "/ask ")).toEqual({
      value: "hi /ask ",
      cursor: 8,
    });
  });
});

describe("buildComposerSlashItems", () => {
  it("includes skills and tools matching the filter", () => {
    const items = buildComposerSlashItems({
      filter: "search",
      manuscripts: [
        {
          id: "doc-search",
          name: "Search docs",
          has_scripts: true,
          scope: "builtin",
        } as never,
      ],
      capabilities: [
        {
          id: "document_search",
          title: "Search documents",
          description: "kb",
        } as never,
      ],
      attachedSkillIds: [],
      attachedToolIds: [],
      includeCommands: true,
    });
    expect(items.some((item) => item.kind === "skill")).toBe(true);
    expect(items.some((item) => item.kind === "tool")).toBe(true);
  });
});
