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
      subtitle: "Forge Changes / Review arrives in a later Code slice",
      keywords: "changes scm git source control vscode",
      aliases: [
        "workbench.view.scm",
        "Changes",
        "Source Control",
        "View: Show Source Control",
      ],
      run: (ctx) => {
        ctx.notice(
          "Changes lands with Forge Review depth — open Review from Code for now.",
        );
        ctx.callbacks.close();
      },
    },
    {
      id: "workbench.action.output.toggleOutput",
      section: "do",
      label: "Output",
      subtitle: "Task Output arrives with streaming execution",
      keywords: "output panel logs vscode",
      aliases: [
        "workbench.action.output.toggleOutput",
        "Output",
        "View: Toggle Output",
      ],
      run: (ctx) => {
        ctx.notice("Output arrives with live task streaming.");
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
