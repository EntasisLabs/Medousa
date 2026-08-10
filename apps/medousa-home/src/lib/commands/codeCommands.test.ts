import { beforeEach, describe, expect, it } from "vitest";

import {
  clearChordOverrides,
  effectiveChordFor,
  listRemappableBindings,
  setChordOverride,
} from "./commandBindings";
import { buildCodeCommands } from "./codeCommands";
import { scoreCommand } from "./score";

describe("code command registry", () => {
  beforeEach(() => clearChordOverrides());

  it("exposes VS Code-familiar aliases for Code journey commands", () => {
    const commands = buildCodeCommands();
    const ids = new Set(commands.map((command) => command.id));
    expect(ids.has("workbench.action.showCommands")).toBe(true);
    expect(ids.has("workbench.action.quickOpen")).toBe(true);
    expect(ids.has("workbench.actions.view.problems")).toBe(true);
    expect(ids.has("workbench.action.terminal.toggleTerminal")).toBe(true);
    expect(ids.has("workbench.action.terminal.focusFind")).toBe(true);
    expect(ids.has("workbench.action.terminal.runSelectedText")).toBe(true);
    expect(ids.has("workbench.action.findInFiles")).toBe(true);
    expect(effectiveChordFor("workbench.action.findInFiles")).toBe("mod:Shift+F");

    const problems = commands.find(
      (command) => command.id === "workbench.actions.view.problems",
    );
    expect(problems?.aliases).toContain("View: Show Problems");
    expect(scoreCommand(problems!, "problems")).toBeGreaterThan(0);
    expect(scoreCommand(problems!, "workbench.actions.view.problems")).toBeGreaterThan(0);
  });

  it("allows overriding a remappable chord", () => {
    expect(effectiveChordFor("workbench.action.showCommands")).toBe("mod:Shift+P");
    setChordOverride("workbench.action.showCommands", "mod:Shift+;");
    expect(effectiveChordFor("workbench.action.showCommands")).toBe("mod:Shift+;");
    const row = listRemappableBindings().find(
      (entry) => entry.commandId === "workbench.action.showCommands",
    );
    expect(row?.overridden).toBe(true);
    clearChordOverrides();
    expect(effectiveChordFor("workbench.action.showCommands")).toBe("mod:Shift+P");
  });
});
