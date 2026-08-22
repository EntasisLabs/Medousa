import { beforeEach, describe, expect, it } from "vitest";

import {
  chordFromKeyboardEvent,
  clearChordOverrides,
  conflictingCommandForChord,
  effectiveChordFor,
  eventMatchesCommandChord,
  listRemappableBindings,
  setChordOverride,
} from "./commandBindings";
import {
  buildCodeCommands,
  buildProjectTaskCommands,
  parseProjectTaskCommandId,
  publishProjectTaskCommandCatalog,
} from "./codeCommands";
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
    expect(ids.has("workbench.action.tasks.runPrimary")).toBe(true);
    expect(ids.has("workbench.action.tasks.build")).toBe(true);
    expect(ids.has("workbench.action.tasks.test")).toBe(true);
    expect(ids.has("testing.runAtCursor")).toBe(true);
    expect(ids.has("workbench.action.tasks.rerunLast")).toBe(true);
    expect(ids.has("workbench.action.tasks.terminate")).toBe(true);
    expect(ids.has("workbench.action.files.saveAll")).toBe(true);
    expect(ids.has("editor.action.formatDocument")).toBe(true);
    expect(ids.has("editor.action.rename")).toBe(true);
    expect(ids.has("workbench.action.files.newFile")).toBe(true);
    expect(ids.has("workbench.action.files.newFolder")).toBe(true);
    expect(ids.has("workbench.action.files.revert")).toBe(true);
    expect(ids.has("workbench.action.files.revealInExplorer")).toBe(true);
    expect(ids.has("medousa.code.repairLanguageSupport")).toBe(true);
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

  it("captures and matches the remappable subset truthfully", () => {
    const event = {
      key: ";",
      metaKey: true,
      shiftKey: true,
      ctrlKey: false,
      altKey: false,
    } as KeyboardEvent;
    expect(chordFromKeyboardEvent(event)).toBe("mod:Shift+;");
    setChordOverride("workbench.action.showCommands", "mod:Shift+;");
    expect(eventMatchesCommandChord(event, "workbench.action.showCommands")).toBe(true);
    expect(
      conflictingCommandForChord(
        "workbench.action.quickOpen",
        "mod:Shift+;",
      ),
    ).toBe("workbench.action.showCommands");
  });

  it("contributes stable active-project commands for every discovered task", () => {
    publishProjectTaskCommandCatalog("work/alpha", [
      {
        id: "npm-dev@nested",
        label: "Web dev server",
        kind: "run",
        argv: ["npm", "run", "dev"],
        root: "apps/web",
        provider: "npm",
        default_rank: 500,
      },
    ]);

    const commands = buildProjectTaskCommands("work/alpha");
    expect(commands).toHaveLength(1);
    expect(commands[0]?.label).toBe("Run Task: Web dev server");
    expect(parseProjectTaskCommandId(commands[0]!.id)).toEqual({
      workId: "work/alpha",
      taskId: "npm-dev@nested",
    });
  });
});
