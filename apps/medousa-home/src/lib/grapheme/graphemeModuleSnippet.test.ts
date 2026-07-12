import { describe, expect, it } from "vitest";
import {
  buildModuleOpCall,
  prepareModuleInsert,
  qualifyModuleOp,
} from "./graphemeModuleSnippet";

describe("qualifyModuleOp", () => {
  it("prefixes short ops with the module id", () => {
    expect(qualifyModuleOp("shell", "run")).toBe("shell.run");
  });

  it("leaves already-qualified ops alone", () => {
    expect(qualifyModuleOp("shell", "shell.run")).toBe("shell.run");
  });
});

describe("buildModuleOpCall", () => {
  it("stubs shell.run with sandbox args", () => {
    expect(buildModuleOpCall("shell.run")).toContain('command: "echo hello"');
    expect(buildModuleOpCall("shell.run")).toContain("network: false");
  });
});

describe("prepareModuleInsert", () => {
  it("inserts a full shell example when the editor is empty", () => {
    const example = `query ShellEcho {
  shell.run(command: "echo hello", network: false, timeout_ms: 5000) { exit_code stdout }
}`;
    const out = prepareModuleInsert("", "shell.run", [example]);
    expect(out).toContain("query ShellEcho");
    expect(out).toContain("shell.run(");
  });
});
