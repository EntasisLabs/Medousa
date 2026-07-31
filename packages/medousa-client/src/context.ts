import type { MedousaContext } from "./types.js";

const MAX_SELECTION_CHARS = 12_000;
const MAX_DIAGNOSTICS = 100;

/**
 * Bound host context before it crosses the plugin/daemon boundary.
 * Host adapters should call this at the edge, before adding context to a turn.
 */
export function boundContext(context: MedousaContext): MedousaContext {
  return {
    ...context,
    selection: context.selection
      ? {
          ...context.selection,
          text: context.selection.text.slice(0, MAX_SELECTION_CHARS),
        }
      : undefined,
    diagnostics: context.diagnostics?.slice(0, MAX_DIAGNOSTICS),
  };
}

/**
 * Temporary host-neutral context rendering for the current daemon contract.
 * A future typed context field can replace this without changing host APIs.
 */
export function contextSupplement(context: MedousaContext): string {
  const bounded = boundContext(context);
  const lines = [`surface: ${bounded.surface}`];
  if (bounded.workspace) lines.push(`workspace: ${bounded.workspace}`);
  if (bounded.file) lines.push(`file: ${bounded.file}`);
  if (bounded.language) lines.push(`language: ${bounded.language}`);
  if (bounded.notePath) lines.push(`note: ${bounded.notePath}`);
  if (bounded.selection?.text) {
    lines.push("selection:", "```", bounded.selection.text, "```");
  }
  if (bounded.diagnostics?.length) {
    lines.push("diagnostics:", ...bounded.diagnostics.map((item) => `- ${item.message}`));
  }
  return `<medousa-context>\n${lines.join("\n")}\n</medousa-context>`;
}
