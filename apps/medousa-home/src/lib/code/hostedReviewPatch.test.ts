import { describe, expect, it } from "vitest";
import { hostedReviewHunks } from "./hostedReviewPatch";

describe("hosted review patches", () => {
  it("keeps both sides' line coordinates across hunks and empty ranges", () => {
    const hunks = hostedReviewHunks("@@ -4,2 +4,2 @@\n context\n-old\n+new\n\\ No newline at end of file\n@@ -20,0 +21,1 @@\n+inserted");
    expect(hunks).toHaveLength(2);
    expect(hunks[0].lines).toEqual([{ kind: "context", content: "context", old_line: 4, new_line: 4 }, { kind: "deletion", content: "old", old_line: 5, new_line: null }, { kind: "addition", content: "new", old_line: null, new_line: 5 }]);
    expect(hunks[1].old_count).toBe(0); expect(hunks[1].lines[0].new_line).toBe(21);
  });
});
