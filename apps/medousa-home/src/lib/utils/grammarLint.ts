/**
 * CodeMirror lint source for the vault grammar check.
 * Diagnostics underline prose issues; suggested fixes appear in the lint
 * hover panel as one-click actions.
 */

import type { Diagnostic } from "@codemirror/lint";
import { linter, lintGutter } from "@codemirror/lint";
import type { EditorView } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import {
  checkGrammar,
  readGrammarSettings,
  type GrammarMatch,
} from "$lib/utils/grammarCheck";

async function grammarDiagnostics(view: EditorView): Promise<Diagnostic[]> {
  const settings = readGrammarSettings();
  if (!settings.enabled) return [];
  const text = view.state.doc.toString();
  if (!text.trim()) return [];

  let matches: GrammarMatch[];
  try {
    matches = await checkGrammar(text, settings);
  } catch {
    // Endpoint offline / unreachable — fail quietly, spellcheck still works.
    return [];
  }

  const docLength = view.state.doc.length;
  return matches
    .filter(
      (m) =>
        m.offset >= 0 &&
        m.offset + m.length <= docLength &&
        m.length > 0,
    )
    .map((m) => ({
      from: m.offset,
      to: m.offset + m.length,
      severity: "warning" as const,
      message: m.message,
      actions: m.replacements.map((replacement) => ({
        name: `→ ${replacement}`,
        apply(target: EditorView, from: number, to: number) {
          target.dispatch({
            changes: { from, to, insert: replacement },
          });
        },
      })),
    }));
}

/** Lint extension set (underlines + hover actions + gutter markers). */
export function grammarLintExtensions(): Extension {
  return [
    linter(grammarDiagnostics, { delay: 1400 }),
    lintGutter(),
  ];
}
