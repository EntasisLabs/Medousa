import { describe, expect, it } from "vitest";
import type { VaultNote } from "$lib/types/vault";
import { buildVaultTree } from "./vaultTree";

function note(path: string): VaultNote {
  return {
    path,
    title: path.split("/").at(-1)?.replace(/\.md$/, "") ?? path,
    byte_size: 1,
    content_hash: path,
    modified_at_utc: "2026-08-20T00:00:00Z",
    created_at_utc: "2026-08-20T00:00:00Z",
    tags: [],
    wikilinks_out: [],
    backlinks: [],
    kind: "note",
  };
}

describe("buildVaultTree", () => {
  it("promotes fallback folders and keeps root files in Loose notes", () => {
    const tree = buildVaultTree(
      [
        note("journal/daily.md"),
        note("client/acme.md"),
        note("client/archive/old.md"),
        note("zeta/reference.md"),
        note("scratch.md"),
      ],
      { showSystemNotes: false },
    );

    expect(tree.map((node) => node.displayLabel ?? node.name)).toEqual([
      "Journal",
      "Inbox",
      "client",
      "zeta",
      "Loose notes",
    ]);
    expect(tree.some((node) => node.spaceId === "other")).toBe(false);

    const client = tree.find((node) => node.name === "client");
    expect(client).toMatchObject({ isFolder: true, dropPrefix: "client/", noteCount: 2 });
    expect(client?.children.map((node) => node.name)).toEqual(["archive", "acme.md"]);

    const loose = tree.find((node) => node.displayLabel === "Loose notes");
    expect(loose).toMatchObject({ noteCount: 1, dropPrefix: "" });
    expect(loose?.children.map((node) => node.path)).toEqual(["scratch.md"]);
  });
});
