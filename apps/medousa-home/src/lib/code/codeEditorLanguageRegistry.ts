import type { Extension } from "@codemirror/state";
import { LanguageSupport } from "@codemirror/language";
import { markdown } from "@codemirror/lang-markdown";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { xml } from "@codemirror/lang-xml";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { yaml } from "@codemirror/lang-yaml";
import {
  graphemeEditorTheme,
  graphemeLanguageSupport,
} from "$lib/grapheme/graphemeEditorTheme";
import { graphemeHostCompletions } from "$lib/grapheme/graphemeHostCompletions";
import { medousaSyntaxHighlighting } from "$lib/syntax/codemirrorSyntaxTheme";
import { vaultMarkdownSyntax } from "$lib/utils/vaultCodeMirror";
import { shellLanguage } from "$lib/code/shellLanguage";

export type CodeEditorLanguageTier = "full" | "highlight";

export type CodeEditorLanguageId =
  | "grapheme"
  | "plaintext"
  | "markdown"
  | "shell"
  | "python"
  | "typescript"
  | "rust"
  | "javascript"
  | "json"
  | "html"
  | "css"
  | "xml"
  | "yaml";

export interface CodeEditorLanguageCapabilities {
  lsp: boolean;
  compile: boolean;
  run: boolean;
  saveToLibrary: boolean;
  addToFlow: boolean;
}

export interface CodeEditorLanguageDefinition {
  id: CodeEditorLanguageId;
  label: string;
  tier: CodeEditorLanguageTier;
  capabilities: CodeEditorLanguageCapabilities;
  /** File suffix hint for snippet tabs (no vault/git wiring). */
  fileExtension?: string;
  aliases?: string[];
}

const FULL: CodeEditorLanguageCapabilities = {
  lsp: true,
  compile: true,
  run: true,
  saveToLibrary: true,
  addToFlow: true,
};

const HIGHLIGHT_ONLY: CodeEditorLanguageCapabilities = {
  lsp: false,
  compile: false,
  run: false,
  saveToLibrary: false,
  addToFlow: false,
};

/** Highlight + Orchestrator LSP when a server is registered for this language. */
const HIGHLIGHT_LSP: CodeEditorLanguageCapabilities = {
  lsp: true,
  compile: false,
  run: false,
  saveToLibrary: false,
  addToFlow: false,
};

const shellLanguageSupport = new LanguageSupport(shellLanguage, [
  medousaSyntaxHighlighting,
]);

const markdownLanguageSupport = markdown({
  codeLanguages: [],
});

export const CODE_EDITOR_LANGUAGES: Record<
  CodeEditorLanguageId,
  CodeEditorLanguageDefinition
> = {
  grapheme: {
    id: "grapheme",
    label: "Grapheme",
    tier: "full",
    capabilities: FULL,
    fileExtension: "grapheme",
  },
  plaintext: {
    id: "plaintext",
    label: "Plain text",
    tier: "highlight",
    capabilities: HIGHLIGHT_ONLY,
    fileExtension: "txt",
    aliases: ["text", "txt"],
  },
  markdown: {
    id: "markdown",
    label: "Markdown",
    tier: "highlight",
    capabilities: HIGHLIGHT_ONLY,
    fileExtension: "md",
    aliases: ["md"],
  },
  shell: {
    id: "shell",
    label: "Shell",
    tier: "highlight",
    capabilities: HIGHLIGHT_ONLY,
    fileExtension: "sh",
    aliases: ["bash", "sh", "zsh"],
  },
  python: {
    id: "python",
    label: "Python",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "py",
    aliases: ["py"],
  },
  typescript: {
    id: "typescript",
    label: "TypeScript",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "ts",
    aliases: ["ts", "tsx"],
  },
  rust: {
    id: "rust",
    label: "Rust",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "rs",
    aliases: ["rs"],
  },
  javascript: {
    id: "javascript",
    label: "JavaScript",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "js",
    aliases: ["js", "jsx", "mjs"],
  },
  json: {
    id: "json",
    label: "JSON",
    tier: "highlight",
    capabilities: HIGHLIGHT_ONLY,
    fileExtension: "json",
  },
  html: {
    id: "html",
    label: "HTML",
    tier: "highlight",
    capabilities: HIGHLIGHT_ONLY,
    fileExtension: "html",
    aliases: ["htm"],
  },
  css: {
    id: "css",
    label: "CSS",
    tier: "highlight",
    capabilities: HIGHLIGHT_ONLY,
    fileExtension: "css",
  },
  xml: {
    id: "xml",
    label: "XML",
    tier: "highlight",
    capabilities: HIGHLIGHT_ONLY,
    fileExtension: "xml",
  },
  yaml: {
    id: "yaml",
    label: "YAML",
    tier: "highlight",
    capabilities: HIGHLIGHT_ONLY,
    fileExtension: "yaml",
    aliases: ["yml"],
  },
};

const ALIAS_INDEX = new Map<string, CodeEditorLanguageId>(
  Object.values(CODE_EDITOR_LANGUAGES).flatMap((def) => [
    [def.id, def.id],
    ...(def.aliases ?? []).map((alias) => [alias, def.id] as const),
  ]),
);

/** Resolve a language id or alias; unknown values fall back to plaintext. */
export function resolveCodeEditorLanguage(
  raw: string | null | undefined,
): CodeEditorLanguageId {
  const key = (raw ?? "").trim().toLowerCase();
  if (!key) return "plaintext";
  return ALIAS_INDEX.get(key) ?? "plaintext";
}

export function getCodeEditorLanguage(
  id: CodeEditorLanguageId | string | null | undefined,
): CodeEditorLanguageDefinition {
  const resolved = resolveCodeEditorLanguage(id);
  return CODE_EDITOR_LANGUAGES[resolved];
}

export function languageSupportsLsp(
  id: CodeEditorLanguageId | string | null | undefined,
): boolean {
  return getCodeEditorLanguage(id).capabilities.lsp;
}

export function languageSupportsCompile(
  id: CodeEditorLanguageId | string | null | undefined,
): boolean {
  return getCodeEditorLanguage(id).capabilities.compile;
}

export function languageSupportsRun(
  id: CodeEditorLanguageId | string | null | undefined,
): boolean {
  return getCodeEditorLanguage(id).capabilities.run;
}

/** CodeMirror extensions for a language tier (never attaches fake LSP). */
export function buildCodeEditorLanguageExtensions(
  id: CodeEditorLanguageId | string | null | undefined,
): Extension[] {
  const def = getCodeEditorLanguage(id);
  switch (def.id) {
    case "grapheme":
      return [graphemeEditorTheme, graphemeLanguageSupport, graphemeHostCompletions()];
    case "markdown":
      return [
        graphemeEditorTheme,
        markdownLanguageSupport,
        vaultMarkdownSyntax,
      ];
    case "shell":
      return [graphemeEditorTheme, shellLanguageSupport];
    case "javascript":
      return [graphemeEditorTheme, javascript()];
    case "typescript":
      return [graphemeEditorTheme, javascript({ typescript: true })];
    case "python":
      return [graphemeEditorTheme, python()];
    case "rust":
      return [graphemeEditorTheme, rust()];
    case "yaml":
      return [graphemeEditorTheme, yaml()];
    case "json":
      return [graphemeEditorTheme, json()];
    case "html":
      return [graphemeEditorTheme, html()];
    case "css":
      return [graphemeEditorTheme, css()];
    case "xml":
      return [graphemeEditorTheme, xml()];
    case "plaintext":
      return [graphemeEditorTheme, medousaSyntaxHighlighting];
    default:
      return [graphemeEditorTheme, medousaSyntaxHighlighting];
  }
}
