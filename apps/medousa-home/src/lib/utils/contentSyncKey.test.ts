import { describe, expect, it } from "vitest";
import { splitContentSyncKey } from "./contentSyncKey";

describe("splitContentSyncKey", () => {
  it("keeps POSIX absolute paths intact", () => {
    expect(splitContentSyncKey("/tmp/loose-note.md:12")).toEqual({
      path: "/tmp/loose-note.md",
      revision: "12",
    });
  });

  it("keeps Windows absolute paths intact", () => {
    expect(splitContentSyncKey("C:/Notes/loose.md:3")).toEqual({
      path: "C:/Notes/loose.md",
      revision: "3",
    });
  });

  it("handles vault-relative keys", () => {
    expect(splitContentSyncKey("notes/a.md:0")).toEqual({
      path: "notes/a.md",
      revision: "0",
    });
  });

  it("fails soft when revision is missing", () => {
    expect(splitContentSyncKey("notes/a.md")).toEqual({
      path: "notes/a.md",
      revision: "0",
    });
  });
});
