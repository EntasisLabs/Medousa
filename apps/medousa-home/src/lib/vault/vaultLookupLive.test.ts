import { describe, expect, it } from "vitest";
import { buildVaultLookupSnapshot } from "$lib/utils/vaultLookup";
import {
  getVaultLookupSnapshot,
  publishVaultLookupSnapshot,
} from "./vaultLookupLive";
import { getVaultNoteBuffer, setVaultNoteBufferPort } from "./vaultNoteBufferPort";
import { emptyNoteBuffer } from "$lib/stores/noteBuffer";

describe("vault lookup live snapshot", () => {
  it("publishes an injected H07 snapshot for feature readers", () => {
    const next = buildVaultLookupSnapshot(
      [{ path: "notes/a.md", title: "A", kind: "note", tags: [], modified_at_utc: "" }],
      7,
      "notes/a.md",
    );
    publishVaultLookupSnapshot(next);
    expect(getVaultLookupSnapshot()).toBe(next);
    expect(getVaultLookupSnapshot().generation).toBe(7);
    expect(getVaultLookupSnapshot().knownPaths.has("notes/a.md")).toBe(true);
  });
});

describe("vault note buffer port", () => {
  it("returns injected buffers and clears when unbound", () => {
    const buffer = emptyNoteBuffer("notes/a.md");
    setVaultNoteBufferPort((path) => (path === buffer.path ? buffer : undefined));
    expect(getVaultNoteBuffer("notes/a.md")).toBe(buffer);
    setVaultNoteBufferPort(null);
    expect(getVaultNoteBuffer("notes/a.md")).toBeUndefined();
  });
});
