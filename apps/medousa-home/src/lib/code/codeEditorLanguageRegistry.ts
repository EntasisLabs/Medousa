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
  | "yaml"
  | "go"
  | "c"
  | "cpp"
  | "csharp"
  | "java"
  | "kotlin"
  | "ruby"
  | "php"
  | "swift"
  | "lua";

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
  go: {
    id: "go",
    label: "Go",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "go",
    aliases: ["golang"],
  },
  c: {
    id: "c",
    label: "C",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "c",
    aliases: ["h"],
  },
  cpp: {
    id: "cpp",
    label: "C++",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "cpp",
    aliases: ["cc", "cxx", "hpp", "hh", "hxx"],
  },
  csharp: {
    id: "csharp",
    label: "C#",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "cs",
    aliases: ["cs", "c#"],
  },
  java: {
    id: "java",
    label: "Java",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "java",
  },
  kotlin: {
    id: "kotlin",
    label: "Kotlin",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "kt",
    aliases: ["kt", "kts"],
  },
  ruby: {
    id: "ruby",
    label: "Ruby",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "rb",
    aliases: ["rb"],
  },
  php: {
    id: "php",
    label: "PHP",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "php",
  },
  swift: {
    id: "swift",
    label: "Swift",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "swift",
  },
  lua: {
    id: "lua",
    label: "Lua",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "lua",
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
      return [graphemeEditorTheme, javascript(), medousaSyntaxHighlighting];
    case "typescript":
      return [
        graphemeEditorTheme,
        javascript({ typescript: true }),
        medousaSyntaxHighlighting,
      ];
    case "python":
      return [graphemeEditorTheme, python(), medousaSyntaxHighlighting];
    case "rust":
      return [graphemeEditorTheme, rust(), medousaSyntaxHighlighting];
    case "yaml":
      return [graphemeEditorTheme, yaml(), medousaSyntaxHighlighting];
    case "json":
      return [graphemeEditorTheme, json(), medousaSyntaxHighlighting];
    case "html":
      return [graphemeEditorTheme, html(), medousaSyntaxHighlighting];
    case "css":
      return [graphemeEditorTheme, css(), medousaSyntaxHighlighting];
    case "xml":
      return [graphemeEditorTheme, xml(), medousaSyntaxHighlighting];
    case "plaintext":
      return [graphemeEditorTheme, medousaSyntaxHighlighting];
    default:
      return [graphemeEditorTheme, medousaSyntaxHighlighting];
  }
}
