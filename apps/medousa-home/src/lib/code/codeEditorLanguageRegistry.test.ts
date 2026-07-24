import { describe, expect, it } from "vitest";
import {
  CODE_EDITOR_LANGUAGES,
  buildCodeEditorLanguageExtensions,
  getCodeEditorLanguage,
  languageSupportsLsp,
  resolveCodeEditorLanguage,
} from "./codeEditorLanguageRegistry";

describe("codeEditorLanguageRegistry", () => {
  it("registers grapheme as full tier with LSP", () => {
    const def = getCodeEditorLanguage("grapheme");
    expect(def.tier).toBe("full");
    expect(def.capabilities.lsp).toBe(true);
    expect(def.capabilities.compile).toBe(true);
    expect(def.capabilities.run).toBe(true);
    expect(languageSupportsLsp("grapheme")).toBe(true);
  });

  it("registers plaintext, markdown, and shell as highlight-only", () => {
    for (const id of ["plaintext", "markdown", "shell"] as const) {
      const def = getCodeEditorLanguage(id);
      expect(def.tier).toBe("highlight");
      expect(def.capabilities.lsp).toBe(false);
      expect(def.capabilities.compile).toBe(false);
      expect(def.capabilities.run).toBe(false);
      expect(languageSupportsLsp(id)).toBe(false);
    }
  });

  it("registers stub languages without editor plug-ins", () => {
    for (const id of ["python", "rust", "typescript"] as const) {
      const def = getCodeEditorLanguage(id);
      expect(def.tier).toBe("stub");
      expect(def.capabilities.lsp).toBe(false);
    }
  });

  it("resolves common aliases", () => {
    expect(resolveCodeEditorLanguage("md")).toBe("markdown");
    expect(resolveCodeEditorLanguage("bash")).toBe("shell");
    expect(resolveCodeEditorLanguage("txt")).toBe("plaintext");
    expect(resolveCodeEditorLanguage("py")).toBe("python");
  });

  it("falls back unknown aliases to plaintext", () => {
    expect(resolveCodeEditorLanguage("not-real")).toBe("plaintext");
  });

  it("builds language extensions without throwing", () => {
    for (const id of Object.keys(CODE_EDITOR_LANGUAGES)) {
      expect(() => buildCodeEditorLanguageExtensions(id)).not.toThrow();
      expect(buildCodeEditorLanguageExtensions(id).length).toBeGreaterThan(0);
    }
  });
});
