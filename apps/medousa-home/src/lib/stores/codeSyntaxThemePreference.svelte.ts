/**
 * Live code / Grapheme syntax theme preference (orthogonal to shell colorTheme).
 */
import {
  readCodeEditorSyntaxTheme,
  writeCodeEditorSyntaxTheme,
} from "$lib/config/codeEditorPreferences";
import {
  getCodeSyntaxTheme,
  resolveCodeSyntaxTheme,
  type CodeSyntaxThemeDefinition,
  type CodeSyntaxThemeId,
} from "$lib/syntax/codeSyntaxThemes";

class CodeSyntaxThemePreference {
  id = $state<CodeSyntaxThemeId>(readCodeEditorSyntaxTheme());

  get theme(): CodeSyntaxThemeDefinition {
    return getCodeSyntaxTheme(this.id);
  }

  set(raw: string): CodeSyntaxThemeId {
    const next = resolveCodeSyntaxTheme(raw);
    writeCodeEditorSyntaxTheme(next);
    this.id = next;
    return next;
  }
}

export const codeSyntaxThemePreference = new CodeSyntaxThemePreference();
