/**
 * Code / workbench commands for Spotlight — stable ids + VS Code-familiar aliases.
 * Editor chrome reacts via `medousa-code-command` window events.
 */

import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { formatCatalogKeys } from "$lib/utils/keyboardShortcutsCatalog";
import { effectiveChordFor } from "./commandBindings";
import type { WorkshopCommand, WorkshopCommandContext } from "./types";

export type CodeCommandDetail = { id: string };

export function dispatchCodeCommand(id: string) {
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent<CodeCommandDetail>("medousa-code-command", {
      detail: { id },
    }),
  );
}

function chordHint(commandId: string, fallback: string): string {
  const chord = effectiveChordFor(commandId);
  if (!chord || chord === "literal:—") return fallback;
  return formatCatalogKeys(chord);
}

export function buildCodeCommands(): WorkshopCommand[] {
  return [
    {
      id: "workbench.action.showCommands",
      section: "do",
      label: "Show All Commands",
      subtitle: `${chordHint("workbench.action.showCommands", "Mod+Shift+P")} — Spotlight command mode`,
      keywords: "command palette spotlight shift+p vscode",
      aliases: [
        "workbench.action.showCommands",
        "Show All Commands",
        "Command Palette",
      ],
      run: (ctx) => {
        commandSpotlight.openCommandPalette();
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.quickOpen",
      section: "do",
      label: "Quick Open",
      subtitle: `${chordHint("workbench.action.quickOpen", "Mod+P")} — file, symbol, or line in Code`,
      keywords: "quick open file go to symbol line vscode",
      aliases: [
        "workbench.action.quickOpen",
        "Quick Open",
        "Go to File",
        "Ctrl+P",
      ],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.quickOpen");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.navigateBack",
      section: "do",
      label: "Go Back",
      subtitle: "Previous Code location (group-aware)",
      keywords: "navigate back history previous location vscode",
      aliases: ["workbench.action.navigateBack", "Go Back", "Back"],
      run: (ctx) => {
        dispatchCodeCommand("workbench.action.navigateBack");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.navigateForward",
      section: "do",
      label: "Go Forward",
      subtitle: "Next Code location (group-aware)",
      keywords: "navigate forward history next location vscode",
      aliases: ["workbench.action.navigateForward", "Go Forward", "Forward"],
      run: (ctx) => {
        dispatchCodeCommand("workbench.action.navigateForward");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.view.explorer",
      section: "go",
      label: "Show Explorer",
      subtitle: "Code project tree in the Library rail",
      keywords: "explorer files tree project vscode",
      aliases: [
        "workbench.view.explorer",
        "Explorer",
        "Show Explorer",
        "View: Show Explorer",
      ],
      run: (ctx) => {
        ctx.navigate("code");
        lmeWorkspace.setExplorerMode("code");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.actions.view.problems",
      section: "do",
      label: "Problems",
      subtitle: "Show project diagnostics in Code",
      keywords: "problems diagnostics errors warnings vscode",
      aliases: [
        "workbench.actions.view.problems",
        "Problems",
        "Show Problems",
        "View: Show Problems",
      ],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.actions.view.problems");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.terminal.toggleTerminal",
      section: "do",
      label: "Toggle Terminal",
      subtitle: `${chordHint("workbench.action.terminal.toggleTerminal", "Mod+`")} — Code terminal dock`,
      keywords: "terminal dock shell console vscode",
      aliases: [
        "workbench.action.terminal.toggleTerminal",
        "Terminal",
        "Toggle Terminal",
        "View: Toggle Terminal",
      ],
      verb: "toggle",
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.terminal.toggleTerminal");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.terminal.focusFind",
      section: "do",
      label: "Find in Terminal",
      subtitle: "Search scrollback in the Code terminal dock",
      keywords: "terminal find search scrollback vscode",
      aliases: [
        "workbench.action.terminal.focusFind",
        "Find in Terminal",
        "Terminal: Find",
      ],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.terminal.focusFind");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.terminal.runSelectedText",
      section: "do",
      label: "Run Selected Text in Terminal",
      subtitle: "Send the editor selection to the workshop shell",
      keywords: "run selection terminal send shell vscode",
      aliases: [
        "workbench.action.terminal.runSelectedText",
        "Run Selected Text",
        "Terminal: Run Selected Text",
      ],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.terminal.runSelectedText");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.view.testing",
      section: "do",
      label: "Tests",
      subtitle: "Toggle the Code tests panel",
      keywords: "tests testing unit vscode",
      aliases: [
        "workbench.view.testing",
        "Testing",
        "Tests",
        "View: Show Testing",
      ],
      verb: "toggle",
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.view.testing");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.tasks.runPrimary",
      section: "do",
      label: "Run Project",
      subtitle: "Run the selected project command",
      keywords: "run project task start launch play",
      aliases: ["Run Project", "Tasks: Run Project", "workbench.action.tasks.runTask"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.tasks.runPrimary");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.tasks.build",
      section: "do",
      label: "Build Project",
      subtitle: "Run the detected project build",
      keywords: "build compile project task",
      aliases: ["Build Project", "Tasks: Run Build Task"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.tasks.build");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.tasks.test",
      section: "do",
      label: "Test Project",
      subtitle: "Run the detected project test command",
      keywords: "test project task unit integration",
      aliases: ["Test Project", "Tasks: Run Test Task"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.tasks.test");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.tasks.verify",
      section: "do",
      label: "Check Project",
      subtitle: "Run the detected project verification command",
      keywords: "check verify lint project task",
      aliases: ["Check Project", "Verify Project", "Tasks: Run Verify Task"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.tasks.verify");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.tasks.rerunLast",
      section: "do",
      label: "Rerun Last Project Command",
      subtitle: "Repeat the exact previous task or targeted test",
      keywords: "rerun repeat last task test build",
      aliases: ["Rerun Last Task", "Tasks: Rerun Last"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.tasks.rerunLast");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.tasks.terminate",
      section: "do",
      label: "Stop Running Project Command",
      subtitle: "Stop the active Forge project task",
      keywords: "stop terminate cancel task process",
      aliases: ["Stop Task", "Terminate Task", "Tasks: Terminate Task"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.tasks.terminate");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.findInFiles",
      section: "do",
      label: "Search in Files",
      subtitle: `${chordHint("workbench.action.findInFiles", "Mod+Shift+F")} — project search`,
      keywords: "search find in files workspace vscode",
      aliases: [
        "workbench.action.findInFiles",
        "Search",
        "Find in Files",
        "Search: Find in Files",
      ],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.findInFiles");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.view.scm",
      section: "do",
      label: "Changes",
      subtitle: "Toggle Forge Changes (branch / upstream / conflicts)",
      keywords: "changes scm git source control vscode",
      aliases: [
        "workbench.view.scm",
        "Changes",
        "Source Control",
        "View: Show Source Control",
      ],
      verb: "toggle",
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.view.scm");
        ctx.callbacks.close();
      },
    },
    {
      id: "git.fetch",
      section: "do",
      label: "Fetch",
      subtitle: "Fetch remotes for the governed working copy",
      keywords: "git fetch remote sync vscode",
      aliases: ["git.fetch", "Fetch", "Git: Fetch"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("git.fetch");
        ctx.callbacks.close();
      },
    },
    {
      id: "git.pull",
      section: "do",
      label: "Pull (fast-forward)",
      subtitle: "Fast-forward only pull into the Forge branch",
      keywords: "git pull ff sync vscode",
      aliases: ["git.pull", "Pull", "Git: Pull"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("git.pull");
        ctx.callbacks.close();
      },
    },
    {
      id: "git.push",
      section: "do",
      label: "Push",
      subtitle: "Push the Forge branch (never force)",
      keywords: "git push remote sync vscode",
      aliases: ["git.push", "Push", "Git: Push"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("git.push");
        ctx.callbacks.close();
      },
    },
    {
      id: "git.sync",
      section: "do",
      label: "Sync",
      subtitle: "Fetch, fast-forward pull, then push when ahead",
      keywords: "git sync fetch pull push vscode",
      aliases: ["git.sync", "Sync", "Git: Sync"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("git.sync");
        ctx.callbacks.close();
      },
    },
    {
      id: "medousa.forge.checkpoint",
      section: "do",
      label: "Seal for Review",
      subtitle: "Checkpoint the working copy and open Review",
      keywords: "seal checkpoint commit review forge vscode",
      aliases: ["medousa.forge.checkpoint", "git.commit", "Seal for Review", "Checkpoint"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("medousa.forge.checkpoint");
        ctx.callbacks.close();
      },
    },
    {
      id: "git.viewHistory",
      section: "do",
      label: "Changes History",
      subtitle: "Commits since the project baseline",
      keywords: "git history log timeline vscode",
      aliases: ["git.viewHistory", "History", "Git: View History"],
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("git.viewHistory");
        ctx.callbacks.close();
      },
    },
    {
      id: "git.blame.toggle",
      section: "do",
      label: "Toggle Blame",
      subtitle: "Show blame for the selected Changes file",
      keywords: "git blame annotate vscode",
      aliases: ["git.blame.toggle", "Blame", "Git: Toggle Blame"],
      verb: "toggle",
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("git.blame.toggle");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.output.toggleOutput",
      section: "do",
      label: "Output",
      subtitle: "Toggle live project task output",
      keywords: "output panel logs vscode",
      aliases: [
        "workbench.action.output.toggleOutput",
        "Output",
        "View: Toggle Output",
      ],
      verb: "toggle",
      run: (ctx) => {
        ctx.navigate("code");
        dispatchCodeCommand("workbench.action.output.toggleOutput");
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.debug.start",
      section: "do",
      label: "Debug",
      subtitle: "Workshop debugger arrives in a later Code slice",
      keywords: "debug start debugger f5 vscode",
      aliases: [
        "workbench.action.debug.start",
        "Debug",
        "Start Debugging",
        "Debug: Start Debugging",
      ],
      run: (ctx) => {
        ctx.notice("Debugging is planned after Tests — not available yet.");
        ctx.callbacks.close();
      },
    },
  ];
}
