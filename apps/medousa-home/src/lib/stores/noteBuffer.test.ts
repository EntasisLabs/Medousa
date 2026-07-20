import { describe, expect, it } from "vitest";
import { cloneNoteBuffer, emptyNoteBuffer } from "./noteBuffer";

describe("noteBuffer", () => {
  it("creates an empty buffer for a path", () => {
    const buffer = emptyNoteBuffer("notes/a.md");
    expect(buffer.path).toBe("notes/a.md");
    expect(buffer.content).toBe("");
    expect(buffer.dirty).toBe(false);
    expect(buffer.contentRevision).toBe(0);
  });

  it("clones without sharing identity", () => {
    const buffer = emptyNoteBuffer("notes/a.md");
    buffer.content = "hello";
    buffer.dirty = true;
    const copy = cloneNoteBuffer(buffer);
    copy.content = "world";
    expect(buffer.content).toBe("hello");
    expect(copy.content).toBe("world");
    expect(copy.dirty).toBe(true);
  });
});
