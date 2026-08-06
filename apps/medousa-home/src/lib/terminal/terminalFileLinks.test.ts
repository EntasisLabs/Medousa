import { describe, expect, it } from "vitest";

import { parseTerminalFileLinks } from "$lib/terminal/terminalFileLinks";

describe("terminal file links", () => {
  it("parses relative path:line:column tokens", () => {
    expect(parseTerminalFileLinks("error src/app.ts:12:3 Unexpected")).toEqual([
      {
        startIndex: 6,
        length: "src/app.ts:12:3".length,
        path: "src/app.ts",
        line: 12,
        column: 3,
      },
    ]);
  });

  it("strips worktree absolute prefixes", () => {
    expect(
      parseTerminalFileLinks(
        "at /work/project/tests/demo.test.ts:9:2",
        "/work/project",
      ),
    ).toEqual([
      {
        startIndex: 3,
        length: "/work/project/tests/demo.test.ts:9:2".length,
        path: "tests/demo.test.ts",
        line: 9,
        column: 2,
      },
    ]);
  });

  it("ignores absolute paths outside the worktree and parent traversal", () => {
    expect(parseTerminalFileLinks("open /etc/passwd:1", "/work/project")).toEqual([]);
    expect(parseTerminalFileLinks("bad ../secret.ts:1")).toEqual([]);
  });
});
