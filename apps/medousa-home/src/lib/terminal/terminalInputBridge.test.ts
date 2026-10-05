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

  it("targets the dock session even when an agent handler registered later", () => {
    const writes: string[] = [];
    const human = registerTerminalInputHandler({ workId: "work", sessionId: () => "human", write: (text) => writes.push(text) });
    const agent = registerTerminalInputHandler({ workId: "work", sessionId: () => "agent", write: () => { throw new Error("must not send to agent"); } });
    expect(writeToTerminal("echo hi", "work", "human")).toBe(true);
    expect(writes).toEqual(["echo hi\n"]);
    human(); agent();
  });

  it("does not substitute another project or session when the target is missing", () => {
    const dispose = registerTerminalInputHandler({ workId: "other", sessionId: () => "other", write: () => { throw new Error("must not substitute"); } });
    expect(writeToTerminal("echo hi", "work")).toBe(false);
    expect(writeToTerminal("echo hi", "other", "missing")).toBe(false);
    dispose();
  });

  it("does not report a send while the requested shell is still connecting", () => {
    let ready = false;
    const writes: string[] = [];
    const dispose = registerTerminalInputHandler({ workId: "work", sessionId: () => "shell", ready: () => ready, write: (text) => writes.push(text) });
    expect(writeToTerminal("echo hi", "work", "shell")).toBe(false);
    ready = true;
    expect(writeToTerminal("echo hi", "work", "shell")).toBe(true);
    expect(writes).toEqual(["echo hi\n"]);
    dispose();
  });
});
