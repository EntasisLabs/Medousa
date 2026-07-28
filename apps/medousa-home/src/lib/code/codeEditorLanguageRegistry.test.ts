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

  it("registers markup languages as highlight-only", () => {
    for (const id of ["json", "html", "css", "xml"] as const) {
      const def = getCodeEditorLanguage(id);
      expect(def.tier).toBe("highlight");
      expect(def.capabilities.lsp).toBe(false);
      expect(def.capabilities.compile).toBe(false);
      expect(def.capabilities.run).toBe(false);
    }
  });

  it("registers python/typescript/rust/javascript as highlight with LSP capability", () => {
    for (const id of ["python", "typescript", "rust", "javascript"] as const) {
      const def = getCodeEditorLanguage(id);
      expect(def.tier).toBe("highlight");
      expect(def.capabilities.lsp).toBe(true);
      expect(def.capabilities.compile).toBe(false);
    }
  });

  it("registers yaml as highlight-only", () => {
    const def = getCodeEditorLanguage("yaml");
    expect(def.tier).toBe("highlight");
    expect(def.capabilities.lsp).toBe(false);
  });

  it("resolves common aliases", () => {
    expect(resolveCodeEditorLanguage("md")).toBe("markdown");
    expect(resolveCodeEditorLanguage("bash")).toBe("shell");
    expect(resolveCodeEditorLanguage("txt")).toBe("plaintext");
    expect(resolveCodeEditorLanguage("py")).toBe("python");
    expect(resolveCodeEditorLanguage("tsx")).toBe("typescript");
    expect(resolveCodeEditorLanguage("yml")).toBe("yaml");
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
