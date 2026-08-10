import type { Extension } from "@codemirror/state";
import { LanguageSupport, StreamLanguage } from "@codemirror/language";
import { markdown } from "@codemirror/lang-markdown";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { xml } from "@codemirror/lang-xml";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { yaml } from "@codemirror/lang-yaml";
import { go } from "@codemirror/lang-go";
import { cpp } from "@codemirror/lang-cpp";
import { java } from "@codemirror/lang-java";
import { php } from "@codemirror/lang-php";
import { svelte } from "codemirror-lang-svelte";
import {
  csharp as csharpMode,
  kotlin as kotlinMode,
} from "@codemirror/legacy-modes/mode/clike";
import { ruby as rubyMode } from "@codemirror/legacy-modes/mode/ruby";
import { lua as luaMode } from "@codemirror/legacy-modes/mode/lua";
import { swift as swiftMode } from "@codemirror/legacy-modes/mode/swift";
import {
  graphemeEditorTheme,
  graphemeLanguageSupport,
} from "$lib/grapheme/graphemeEditorTheme";
import { graphemeHostCompletions } from "$lib/grapheme/graphemeHostCompletions";
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
  | "tsx"
  | "rust"
  | "javascript"
  | "jsx"
  | "svelte"
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
  /**
   * Language id sent to the workshop coding engine / LSP.
   * Defaults to `id` when omitted.
   */
  lspLanguageId?: string;
  /** Optional package id that Repair installs for this language (HCP-3B). */
  packageId?: string;
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

const shellLanguageSupport = new LanguageSupport(shellLanguage);

const csharpLanguageSupport = new LanguageSupport(StreamLanguage.define(csharpMode));
const kotlinLanguageSupport = new LanguageSupport(StreamLanguage.define(kotlinMode));
const rubyLanguageSupport = new LanguageSupport(StreamLanguage.define(rubyMode));
const luaLanguageSupport = new LanguageSupport(StreamLanguage.define(luaMode));
const swiftLanguageSupport = new LanguageSupport(StreamLanguage.define(swiftMode));

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
    packageId: "langservers",
  },
  typescript: {
    id: "typescript",
    label: "TypeScript",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "ts",
    aliases: ["ts"],
    packageId: "langservers",
  },
  tsx: {
    id: "tsx",
    label: "TSX",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "tsx",
    aliases: ["tsx"],
    lspLanguageId: "typescript",
    packageId: "langservers",
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
    aliases: ["js", "mjs"],
    packageId: "langservers",
  },
  jsx: {
    id: "jsx",
    label: "JSX",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "jsx",
    aliases: ["jsx"],
    lspLanguageId: "javascript",
    packageId: "langservers",
  },
  svelte: {
    id: "svelte",
    label: "Svelte",
    tier: "highlight",
    capabilities: HIGHLIGHT_LSP,
    fileExtension: "svelte",
    aliases: ["svelte"],
    packageId: "langservers",
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

/** Language id used for workshop LSP sessions and didOpen payloads. */
export function codeEditorLspLanguageId(
  id: CodeEditorLanguageId | string | null | undefined,
): string {
  const def = getCodeEditorLanguage(id);
  return def.lspLanguageId ?? def.id;
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

/** Optional package Repair should install for this editor language. */
export function languageRepairPackageId(
  id: CodeEditorLanguageId | string | null | undefined,
): string | null {
  return getCodeEditorLanguage(id).packageId ?? null;
}

/** CodeMirror extensions for a language tier (never attaches fake LSP).
 * Syntax colors are owned by CodeMirrorHost’s syntax theme compartment. */
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
    case "jsx":
      return [graphemeEditorTheme, javascript({ jsx: true })];
    case "typescript":
      return [graphemeEditorTheme, javascript({ typescript: true })];
    case "tsx":
      return [
        graphemeEditorTheme,
        javascript({ typescript: true, jsx: true }),
      ];
    case "svelte":
      return [graphemeEditorTheme, svelte()];
    case "python":
      return [graphemeEditorTheme, python()];
    case "rust":
      return [graphemeEditorTheme, rust()];
    case "go":
      return [graphemeEditorTheme, go()];
    case "c":
    case "cpp":
      return [graphemeEditorTheme, cpp()];
    case "java":
      return [graphemeEditorTheme, java()];
    case "php":
      return [graphemeEditorTheme, php()];
    case "csharp":
      return [graphemeEditorTheme, csharpLanguageSupport];
    case "kotlin":
      return [graphemeEditorTheme, kotlinLanguageSupport];
    case "ruby":
      return [graphemeEditorTheme, rubyLanguageSupport];
    case "lua":
      return [graphemeEditorTheme, luaLanguageSupport];
    case "swift":
      return [graphemeEditorTheme, swiftLanguageSupport];
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
      return [graphemeEditorTheme];
    default:
      return [graphemeEditorTheme];
  }
}
