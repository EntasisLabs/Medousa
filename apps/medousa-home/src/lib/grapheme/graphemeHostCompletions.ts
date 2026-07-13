/**
 * Host-module completions for Medousa Grapheme modules that are not in
 * grapheme-signatures (shell.*, medousa.*). Complements the stock LSP via
 * languageData — does not override LSP / default autocomplete sources.
 */

import { EditorState } from "@codemirror/state";
import type { Completion, CompletionContext } from "@codemirror/autocomplete";

const HOST_OPS: Completion[] = [
  {
    label: "shell.run",
    type: "function",
    detail: "sandboxed OS command",
    apply: 'shell.run(command: "echo hello", network: false, timeout_ms: 5000)',
  },
  {
    label: "shell.status",
    type: "function",
    detail: "sandbox probe",
    apply: "shell.status()",
  },
  {
    label: "medousa.digest",
    type: "function",
    detail: "digest payload",
    apply: 'medousa.digest(text: "summarize this turn")',
  },
  {
    label: "medousa.synthesize",
    type: "function",
    detail: "synthesize result",
    apply: "medousa.synthesize(payload: {})",
  },
  {
    label: "medousa.deliver",
    type: "function",
    detail: "deliver result",
    apply: "medousa.deliver(payload: {})",
  },
];

function hostModuleCompletions(context: CompletionContext) {
  const word = context.matchBefore(/[A-Za-z_][\w.]*/);
  if (!word || (word.from === word.to && !context.explicit)) {
    return null;
  }
  const needle = word.text.toLowerCase();
  const options = HOST_OPS.filter(
    (op) =>
      op.label.toLowerCase().startsWith(needle) ||
      op.label.toLowerCase().includes(needle),
  );
  if (options.length === 0) return null;
  return {
    from: word.from,
    options,
    validFor: /^[\w.]*$/,
  };
}

/** CodeMirror extension — host module quick completions beside LSP. */
export function graphemeHostCompletions() {
  return EditorState.languageData.of(() => [{ autocomplete: hostModuleCompletions }]);
}
