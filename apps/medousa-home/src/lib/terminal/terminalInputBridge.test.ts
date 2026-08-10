import { describe, expect, it } from "vitest";

import {
  registerTerminalInputHandler,
  writeToTerminal,
} from "$lib/terminal/terminalInputBridge";

describe("terminal input bridge", () => {
  it("routes text to the matching workId handler", () => {
    const writes: string[] = [];
    const disposeA = registerTerminalInputHandler({
      workId: "work-a",
      write: (text) => writes.push(`a:${text}`),
    });
    const disposeB = registerTerminalInputHandler({
      workId: "work-b",
      write: (text) => writes.push(`b:${text}`),
    });
    expect(writeToTerminal("echo hi", "work-b")).toBe(true);
    expect(writes).toEqual(["b:echo hi\n"]);
    disposeA();
    disposeB();
  });

  it("returns false when no terminal is registered", () => {
    expect(writeToTerminal("noop")).toBe(false);
  });
});
